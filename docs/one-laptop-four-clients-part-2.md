<!--
PINNED — DO NOT PUBLISH YET.
This is the parked technical deep-dive for Part 2. Content below is the material
cut from the original single post, lightly reorganised. Needs a proper edit pass
before publishing: an intro that stands alone, a link back to Part 1, and a
section covering the Node/Python toolchain switching promised in Part 1.
-->

# One Laptop, Four Clients (Part 2): The Deep Dive

### About 40 lines of zsh that give every terminal its own identity. Part 1 was the why; this is the how.

---

## Building it

The whole thing lives in `~/.profiles.d/`, one file per context:

```
~/.profiles.d/
  normal.zsh    # personal — the base everything else builds on
  globex.zsh    # client
  acme.zsh      # client
```

A profile is a plain shell script. There's no framework, no DSL, no plugin. You `source` it and it exports things. If you can read `export FOO=bar` you can read all of this, and more importantly you can debug it at 4pm with `echo $AWS_PROFILE`.

### Step 1: git identity, without ever touching ~/.gitconfig

Git has honoured these environment variables since forever, and they beat anything in your config files:

```sh
export GIT_AUTHOR_NAME="Jane Doe"
export GIT_AUTHOR_EMAIL="jane.doe@acme.com"
export GIT_COMMITTER_NAME="Jane Doe"
export GIT_COMMITTER_EMAIL="jane.doe@acme.com"
```

Set those and every commit made in this shell is authored correctly, no matter which directory you're in, no matter what `~/.gitconfig` says. Nothing on disk changed, so the terminal next to you is unaffected.

**A gotcha that will confuse you once and then never again:** `git config user.email` will still print the value from `~/.gitconfig`. That's not a bug. `git config` reads *config files*, and the vars above aren't config — they're authorship overrides applied at commit time. Don't trust `git config user.email` to tell you who you're about to commit as. Trust `git var GIT_AUTHOR_IDENT`, which accounts for the env:

```sh
$ git var GIT_AUTHOR_IDENT
Jane Doe <jane.doe@acme.com> 1755734400 +0100
```

Now, signing keys. There's no `GIT_SIGNINGKEY` variable, which is annoying, and it's where most people give up and start editing `~/.gitconfig` again. Don't. Git 2.31 added a much less well-known escape hatch that lets you inject *arbitrary config* through the environment:

```sh
export GIT_CONFIG_COUNT=1
export GIT_CONFIG_KEY_0="user.signingkey"
export GIT_CONFIG_VALUE_0="ABC123..."
```

You set `GIT_CONFIG_COUNT` to how many entries you're passing, then number them from zero. Git treats them at command-line precedence, so they beat every config file. Want a second one? Bump the count and add `KEY_1`/`VALUE_1`.

This is the single most useful thing in this post. It means **any** git setting can be made per-shell. Different signing key per client, different `core.sshCommand`, different `commit.gpgsign` — all of it, without a single write to disk.

### Step 2: kube, the actual hard one

Back to `kubectl config use-context` writing to a shared file. You cannot stop it writing. What you *can* do is change which file it writes to, using `$KUBECONFIG`.

Give each shell its own copy:

```sh
# Give THIS shell its own kubeconfig so `kubectl config use-context` in one
# terminal never leaks into another. Seeds a per-profile copy on first use.
_kube_isolate() {
  local name="${1:-default}" base kc="$HOME/.kube/config.${1:-default}"
  if [ ! -f "$kc" ]; then
    # Resolve ~/.kube/config even if it's a symlink.
    base=$(readlink -f "$HOME/.kube/config" 2>/dev/null || echo "$HOME/.kube/config")
    [ -f "$base" ] && { mkdir -p "$HOME/.kube"; cp "$base" "$kc" 2>/dev/null || true; }
  fi
  export KUBECONFIG="$kc"
}
```

First time you use the `acme` profile it copies your base config to `~/.kube/config.acme` and points `KUBECONFIG` at it. From then on, that shell reads and writes its own file. `use-context` still mutates global state, it's just that "global" now means "this profile", and the blast radius is one terminal.

The profile then does:

```sh
command -v _kube_isolate >/dev/null 2>&1 && _kube_isolate acme
kubectl config use-context acme >/dev/null 2>&1 || true
```

That `|| true` matters more than it looks. You want a profile to load cleanly on a machine where that cluster doesn't exist — a fresh laptop, a CI box, a container. A profile that hard-fails because a kube context is missing is a profile you'll stop using.

### Step 3: unset before you set (the bit everyone forgets)

Here's the bug that took me longest to notice. Switch from client A to client B:

```sh
profile acme     # exports JIRA_ACME_API_TOKEN
profile globex   # exports AUTHENTIK_TOKEN
```

Now check your environment. `JIRA_ACME_API_TOKEN` is **still there**. Sourcing a file only sets what that file mentions. Client A's token is now sitting in a shell you're using to run client B's tooling, and it'll get inherited by every process you launch from it.

So the switcher wipes the slate first:

```sh
_profile_reset() {
  unset GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL \
        GIT_CONFIG_COUNT GIT_CONFIG_KEY_0 GIT_CONFIG_VALUE_0 \
        JIRA_ACME_API_TOKEN AUTHENTIK_TOKEN AUTHENTIK_BOOTSTRAP_TOKEN \
        AUTHENTIK_PASSWORD ACME_ARGOCD_COMMON_PATH KUBECONFIG 2>/dev/null
}
```

Yes, it's a hardcoded list, and yes, that means you have to remember to add to it when a profile grows a new variable. I've thought about prefixing every managed variable with something greppable so I could wipe them generically. Haven't done it. The explicit list has the advantage of being obvious when you read it.

### Step 4: inheritance is just `source`

Profiles share most of their content, so let them inherit. There's no mechanism for this — you just source the parent at the top and override below it:

```sh
# globex — inherits from `normal`, then overrides.
source "${HOME}/.profiles.d/normal.zsh"

export AWS_PROFILE="globex"
export AWS_REGION="eu-west-2"
export AWS_DEFAULT_REGION="$AWS_REGION"

command -v _kube_isolate >/dev/null 2>&1 && _kube_isolate globex
kubectl config use-context globex >/dev/null 2>&1 || true

export AUTHENTIK_URL="https://sso.globex.dev"
export AUTHENTIK_TOKEN="..."

# Identity stays personal here. My acme profile overrides it instead.

# Set LAST, or you inherit normal's value and the prompt lies to you.
export DEV_PROFILE="globex"
```

That last line is a real trap. `normal.zsh` sets `DEV_PROFILE=normal`, so if you set yours before the `source` line, or forget it entirely, your prompt cheerfully claims you're on your personal profile while you're pointed at a client's cluster. Set the marker last.

The difference between my two client profiles is just how far down they override: `globex` keeps my personal git identity and only changes cloud and tokens, while `acme` also overrides name, email and signing key. Same pattern, different depth.

### Step 5: the switcher

```sh
profile() {
  local dir="$HOME/.profiles.d" name="${1:-}"
  [ -d "$dir" ] || { echo "no ~/.profiles.d"; return 1; }

  # No argument? fzf picker.
  if [ -z "$name" ]; then
    name=$(ls "$dir"/*.zsh 2>/dev/null | xargs -n1 basename | sed 's/\.zsh$//' \
      | fzf --prompt='client profile > ' --height=40% --reverse) || return
  fi
  [ -n "$name" ] || return
  [ -f "$dir/$name.zsh" ] || { echo "no profile: $name"; return 1; }

  _profile_reset
  source "$dir/$name.zsh"   # the profile handles its own inheritance

  echo "⛭ profile: $DEV_PROFILE  (git=$GIT_AUTHOR_EMAIL  aws=$AWS_PROFILE  kube=$(kubectl config current-context 2>/dev/null))"
}
```

That's it. Forty-odd lines across the whole system.

```sh
$ profile acme
⛭ profile: acme  (git=jane.doe@acme.com  aws=acme-aws  kube=acme)
```

Typing `profile` with no argument gives you an fzf picker. Tab-completion for free, since the files are just files.

New shells load `normal` automatically and nothing else:

```sh
[ -f "$HOME/.profiles.d/normal.zsh" ] && source "$HOME/.profiles.d/normal.zsh"
```

I deliberately don't prompt on shell start. If opening a tmux pane makes you answer a question, you will start dreading opening tmux panes.

<!-- TODO for Part 2 edit pass: add a step here on per-profile language toolchains
     (PYENV_VERSION, NPM_CONFIG_USERCONFIG for per-client registries, PIP_INDEX_URL,
     PATH prepends) — promised in Part 1. -->

## Make the state visible

Per-shell state is great right up until you forget which shell is which. So the prompt shows it. I use starship, and there's a custom module that just reads the marker variable:

```toml
[custom.profile]
command = 'printf "%s" "${DEV_PROFILE:-normal}"'
when = true
shell = ["sh", "--norc"]
format = "[ 󱃖 $output ]($style)"
style = "fg:base bg:mauve"
```

And then the bit I'd fit even if I skipped everything else in this post — colour the kube context by how much trouble it can cause:

```toml
[[kubernetes.contexts]]
context_pattern = ".*prod.*"
style = "fg:base bg:red"
symbol = "🚨 ☸ "

[[kubernetes.contexts]]
context_pattern = ".*(staging|stage|uat).*"
style = "fg:base bg:yellow"

[[kubernetes.contexts]]
context_pattern = ".*(dev|develop|sandbox|local|kind|minikube).*"
style = "fg:base bg:green"
```

Green pane, yellow pane, and a pane that has gone bright red. You stop reading it consciously after a week and start noticing it peripherally, which is the whole point. It's a hardware interlock for your hands.

## Where the tokens come from

The profiles reference things like `AUTHENTIK_TOKEN`. Those aren't in git.

The whole setup is managed with [chezmoi](https://chezmoi.io), which stores dotfiles as Go templates and renders them into your home directory on `chezmoi apply`. So the profile files in the repo are actually `.tmpl` files with holes in them:

```sh
export AUTHENTIK_TOKEN="{{ .secrets.authentik_bootstrap_token | default "" }}"
```

At apply time a script pulls the real values from Bitwarden into a local, git-ignored `secrets.yaml`, and chezmoi renders them in.

**Be honest about the tradeoff here**, because it's a real one: this bakes secrets as plaintext into files in your home directory. I chose that deliberately, because the alternative is calling out to a secret manager on every shell start, which is slow, and which means your terminal doesn't work on a train. Baked secrets mean shells work offline and start instantly.

If you'd rather not make that trade, `rbw` (an unofficial Bitwarden client with an agent) lets you fetch at apply time from an unlocked agent, or you can go further and resolve at runtime. Pick your poison knowingly. What you should *not* do is what I was doing before any of this: a `~/.zshrc` with live API tokens in it, committed to a repo. `gitleaks` in a pre-commit hook now stops me repeating that.

## Test your dotfiles. Genuinely

This felt over the top until the first time it caught something. There's a Dockerfile and a test script that applies the whole config on Linux and asserts the behaviour:

```sh
# globex inherits normal's identity, overrides aws
out=$(zsh -c 'source $HOME/.profiles.d/globex.zsh; echo "$DEV_PROFILE|$GIT_AUTHOR_EMAIL|$AWS_PROFILE"')
```

and, my favourite, an end-to-end check that makes an actual commit and inspects who git thinks wrote it:

```sh
tmp=$(mktemp -d); cd "$tmp" && git init -q
zsh -c 'source $HOME/.profiles.d/acme.zsh; git commit -q --allow-empty -m t; git log -1 --format="%ae"'
# expects jane.doe@acme.com
```

Plus one that opens two independent shells, loads a different profile in each, and asserts they ended up with different `KUBECONFIG` values. That's the property the entire design rests on, so it gets an assertion.

Dotfiles are software. They have edge cases, they regress, and they fail at the worst possible moment because you only exercise them when you're doing something else. Fifteen minutes of test harness is cheap.

## What's still not great

- **The unset list is manual.** Add a variable to a profile, forget to add it to `_profile_reset`, and it leaks on switch. A naming convention would fix this properly. It's on the list.
- **It's per-shell, so it doesn't follow you.** Long-running processes started before a switch keep the old environment. A GUI app launched from Spotlight knows nothing about any of this. That's inherent to the approach, not a bug I can fix.
- **Baked plaintext secrets**, discussed above. Correct for my threat model. Check it against yours.
- **`_kube_isolate` copies the kubeconfig on first use**, so if you add a cluster to your base config later, existing per-profile copies won't see it. Delete `~/.kube/config.<name>` and it reseeds. Fine, but a sharp edge.

## Steal this

Start with one file, `~/.profiles.d/work.zsh`, containing four `export` lines and a `source` in your `.zshrc`. Grow it when it annoys you. That's how this one got here.

The full setup is in [my dotfiles repo](https://github.com/planesailingio/hats) if you want to read the whole thing.
