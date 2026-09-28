# hats for VS Code

Keeps a window's VS Code profile in step with the hat its folder belongs to.

Each hat has a VS Code profile of the same name, created by `hats` with its own
`settings.json` and everything else, extensions included, shared with the
default profile. VS Code remembers which profile a folder was last opened with,
so this extension only has to notice when the two disagree.

## What it does

- Shows the window's hat in the status bar, highlighted when it does not match
  the folder's.
- Offers to switch profile when a folder asks for a different hat.
- Warns whenever a hat is detected (a `.hat` file in the folder, a `hats.hat`
  setting, or an active hat in the environment), pointing at VS Code's profile
  manager for creating and managing the hat's profile.
- Says when a hat has no profile yet, and how to create one.

## Setting up a folder

Put the hat in the folder's `.vscode/settings.json`:

```json
{
  "hats.hat": "acme"
}
```

Opening that folder under another profile offers **Switch Profile...**. Once
picked, VS Code reopens the folder with that profile from then on, and opening
it with `code` from a shell wearing the hat does the same without asking.

## Why it cannot switch on its own

VS Code has no extension API for profiles. The command that switches them takes
no argument and always shows its own picker, so a profile is always chosen by
the person, never by an extension. Creating a profile is hats' job, and can
only be done while VS Code is closed: VS Code keeps its profile list in memory
and rewrites it whenever anything changes.

## Commands

| Command | Does |
| --- | --- |
| `hats: Switch Profile...` | Opens VS Code's profile picker |
| `hats: Set This Profile's Hat...` | Records which hat this profile is for, for a profile made by hand |

## Installing

Each hats release attaches the packaged extension as `hats-<version>.vsix`;
install it with `code --install-extension hats-<version>.vsix`. It goes into
the default profile, which every hat profile shares its extensions with. A
marketplace listing is planned.
