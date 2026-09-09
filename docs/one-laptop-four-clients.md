# One Laptop, Four Clients (Part 1)

### Every tool on your machine remembers who you are — and they all remember it somewhere different. This is the story and the why. Part 2 is the how.

---

It's 4pm. I have two terminal panes open. The left one is a client's staging cluster, the right one is my homelab. I run `kubectl delete pod` in what I am extremely confident is the left pane.

It was the left pane. That's not the problem. The problem is that twenty minutes earlier I ran `kubectl config use-context homelab` in the *right* pane, and that command didn't do what I thought it did.

It didn't change that shell. It edited a file. `~/.kube/config`, shared by every terminal on the machine. My left pane had been pointing at my homelab the whole time, quietly, and the prompt was telling me the truth if I'd looked.

I run four working contexts on one laptop — personal, plus three clients — and this post is the story of how I stopped trusting my terminal, why that turned out to be a much bigger productivity problem than I'd admitted, and what I built to get the trust back. Switching contexts is now one command that moves git identity, cloud credentials, kube context, API tokens, language toolchains and the prompt together, per terminal.

Nothing here is clever. That's sort of the point.

## This is not a Kubernetes problem

Kubernetes is just where it bit me. The same shape of problem is lying in wait for almost every tool you use, whatever kind of developer you are:

- **git** reads your name and email from `~/.gitconfig`. Commit to a client's repo with your personal email and you've leaked your identity into their history forever — rewriting it means rewriting theirs.
- **npm** reads `~/.npmrc`. If a client runs a private registry, that file decides where your packages install from and where an accidental `npm publish` goes.
- **pip** has an index URL. Point it at a client's Artifactory once and forget, and your personal projects quietly resolve dependencies through their infrastructure.
- **Node and Python versions.** Client A is pinned to Node 18 and Python 3.10 because that's what their CI runs. Client B is on Node 22. Your laptop has an opinion of its own.
- **Tokens.** The Jira token, the internal API key, the SSO session. They live in your environment, and your environment doesn't know which client you're being right now.

Different tools, one underlying question — *who am I at this moment?* — and no two of them agree on where the answer lives. Some read an environment variable, which is naturally per-terminal. Some read a file in your home directory, which is shared by everything on the machine. `kubectl config use-context` looks like a mode switch. It isn't. It's an edit to global state, and so is `git config --global user.email`, and so is whatever `nvm use` last did to your default.

## The real cost isn't the mistakes

The near-misses make good war stories, but they're rare. The daily cost is quieter and much bigger: the checking.

Before anything with consequences, the ritual. `git config user.email`. `echo $AWS_PROFILE`. `kubectl config current-context`. `node --version`, if I'd been anywhere near the other client that morning. Every context switch came with a little checklist, and every checklist gets skipped eventually — usually at 4pm, usually when you're already holding three other things in your head.

That's the actual tax. Not the incident, the *vigilance*. Context switching between clients is expensive enough — different codebases, different Slack workspaces, different mental models — without your own terminal being one more thing you have to interrogate before you trust it. I was spending real attention, many times a day, on a question a computer should be answering for me.

## What people normally do, and why I stopped

**`includeIf gitdir:` in `.gitconfig`.** The standard advice, and good advice as far as it goes: any repo under `~/git/clientA/` gets that client's identity automatically. But it only knows about directories, so it breaks the moment you clone something somewhere unusual or `cd` into `/tmp` to try something. And it does nothing for your kube context, your AWS profile, your npm registry or your tokens. It solves a quarter of the problem.

**direnv.** Same idea generalised — a `.envrc` per directory, loaded when you `cd` in. Genuinely useful, and if your contexts map cleanly onto directories it may be all you need. Mine don't. I'm one person being four people, and which client I'm being is a property of *me right now*, not of the folder I happen to be standing in.

**`kubectx` / `kubie`.** Good tools. `kubie` in particular solves the kube half properly. I used it for ages. But then I had one tool for kube, an `includeIf` block for git, an alias for AWS, and a `.env` file for tokens — four mechanisms that can disagree with each other. When I switch clients I want *one* thing to happen.

**Separate macOS user accounts.** Total isolation, genuinely correct, and I lasted about a week. Logging out to answer a Slack message is not a life.

## One command

So I built the boring thing. A hat, per context, that sets *everything* — git identity, signing key, AWS profile, kube context, tokens, registries, toolchain versions — as environment scoped to a single terminal. Switching looks like this:

```sh
$ hat acme
⛭ hat: acme  (git=jane.doe@acme.com  aws=acme-aws  kube=acme)
```

That's the whole interface. One command, and one line back telling you who you now are. Run it with no argument and you get a fuzzy-searchable picker. Run it in the pane next door and that pane becomes someone else, and the two never interfere — the property the entire design rests on is that **a context switch in one terminal cannot leak into another**.

The other half is making the state impossible to ignore. The active hat sits in my prompt, and the kube context is coloured by how much trouble it can cause: green for dev, yellow for staging, and production is bright red with a warning sign. You stop reading it consciously after a week and start noticing it peripherally, which is the whole point. It's a hardware interlock for your hands.

The checking ritual is gone. Not shortened — gone. The answer to "who am I right now" is printed at the front of every prompt, per terminal, and I haven't run the checklist in months.

It started as a pile of shell script in my dotfiles, and I've since wrapped it into a small standalone CLI so it doesn't need the rest of my setup:

```sh
brew install planesailingio/tools/hats
```

You don't need to be a platform engineer for any of this to apply. If you've ever committed with the wrong email, published to the wrong registry, or run a deploy with the wrong credentials — or just lost ten minutes re-establishing which hat you were wearing — you have this problem, whether or not it's bitten you yet.

## The idea is four lines long

If you take nothing else away:

1. Put per-context state in **environment variables**, never in shared files under `~`.
2. For any tool that insists on a file, give each context **its own copy of the file** and point an environment variable at it.
3. **Unset before you set**, or the last context leaks into the next one.
4. **Put the active context in your prompt**, and colour production red.

Everything I built is an elaboration of those four rules, and you could start this afternoon with one file containing four `export` lines. That's how mine started.

## Part 2

The how is its own post: the obscure git environment variables that make *any* git setting per-shell without touching `~/.gitconfig`, the trick that tames `kubectl`'s shared config file, the variable-leak bug that took me longest to notice, where the secrets live, and why my dotfiles have a test suite that makes real commits inside Docker to check who git thinks wrote them.

The full setup is in [my dotfiles repo](https://github.com/planesailingio/hats) if you'd rather just read the source.
