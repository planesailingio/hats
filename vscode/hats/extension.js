// hats — keeps a window's VS Code profile in step with its folder's hat.
//
// VS Code gives an extension no way to create or switch a profile on its own:
// `workbench.profiles.actions.switchProfile` takes no arguments and always
// shows its own picker, and the profile list is only writable while VS Code is
// closed. So this extension does the two things it can: it notices when the
// window's profile does not match the hat a folder asks for and offers the
// picker, and it says when a hat has no profile yet.
//
// Which hat a profile is for is read from `hats.hat` in the profile's own
// settings, which `hats` writes when it creates the profile. The folder's hat
// is the same setting in `.vscode/settings.json`. `inspect()` keeps the two
// apart even though the folder's value would otherwise win.

const vscode = require('vscode');
const { execFile } = require('child_process');
const os = require('os');
const path = require('path');

const SETTING = 'hats.hat';
const SKIP_FOLDER = 'hats.skipFolder';
const SKIP_MISSING = 'hats.skipMissingProfiles';

let log;
let status;
/** Hats already asked about in this window, so a decline is not re-asked. */
const asked = new Set();

function activate(context) {
  log = vscode.window.createOutputChannel('hats');
  context.subscriptions.push(log);

  status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  status.command = 'hats.switchProfile';
  context.subscriptions.push(status);

  context.subscriptions.push(
    vscode.commands.registerCommand('hats.switchProfile', switchProfile),
    vscode.commands.registerCommand('hats.setProfileHat', () => setProfileHat(context)),
    vscode.workspace.onDidChangeConfiguration((e) => {
      // Fires when the profile changes too, which is how a switch is noticed.
      if (e.affectsConfiguration(SETTING)) {
        check(context);
      }
    }),
    vscode.workspace.onDidChangeWorkspaceFolders(() => check(context)),
  );

  watchHatsConfig(context);
  check(context);
  checkProfilesExist(context);
}

/** The hat this window's profile is for, and the one its folder asks for. */
function hats() {
  const folder = vscode.workspace.workspaceFolders?.[0];
  const found = vscode.workspace
    .getConfiguration('hats', folder?.uri)
    .inspect('hat');
  const value = (v) => (v && String(v).trim()) || undefined;
  return {
    profile: value(found?.globalValue),
    folder: value(found?.workspaceFolderValue) || value(found?.workspaceValue),
  };
}

/** Show the hat in the status bar, and offer to switch when it is wrong. */
async function check(context) {
  try {
    const { profile, folder } = hats();

    if (profile || folder) {
      const mismatch = folder && profile !== folder;
      status.text = `$(person) ${profile || 'no hat'}`;
      status.tooltip = mismatch
        ? `This window's profile is for ${profile || 'no hat'}, but this folder wears ${folder}.`
        : `VS Code profile for hat ${profile}`;
      status.backgroundColor = mismatch
        ? new vscode.ThemeColor('statusBarItem.warningBackground')
        : undefined;
      status.show();
    } else {
      status.hide();
    }

    if (!folder || profile === folder) return;
    if (context.workspaceState.get(SKIP_FOLDER) === folder) return;
    if (asked.has(folder)) return;
    asked.add(folder);

    const switchTo = 'Switch Profile...';
    const mark = "Set This Profile's Hat...";
    const never = "Don't ask for this folder";
    const buttons = profile ? [switchTo, never] : [switchTo, mark, never];
    const answer = await vscode.window.showInformationMessage(
      `This folder wears the hat ${folder}, but this window uses the ${profile || 'default'} profile.`,
      ...buttons,
    );

    if (answer === switchTo) await switchProfile();
    else if (answer === mark) await setProfileHat(context);
    else if (answer === never) await context.workspaceState.update(SKIP_FOLDER, folder);
  } catch (e) {
    log.appendLine(`check: ${e}`);
  }
}

/** VS Code's own profile picker: it takes no argument, so it must be chosen. */
async function switchProfile() {
  await vscode.commands.executeCommand('workbench.profiles.actions.switchProfile');
}

/** Record which hat this profile is for, for a profile made by hand. */
async function setProfileHat(context) {
  try {
    const names = await hatNames();
    const pick = names.length
      ? await vscode.window.showQuickPick(names, { title: 'Which hat is this profile for?' })
      : await vscode.window.showInputBox({ title: "This profile's hat" });
    if (!pick) return;

    await vscode.workspace
      .getConfiguration('hats')
      .update('hat', pick, vscode.ConfigurationTarget.Global);
    asked.clear();
    await check(context);
  } catch (e) {
    log.appendLine(`setProfileHat: ${e}`);
    vscode.window.showErrorMessage(`hats: could not set the profile's hat: ${e}`);
  }
}

/** Tell the user about hats with no profile: only hats can create one. */
async function checkProfilesExist(context) {
  try {
    if (context.globalState.get(SKIP_MISSING)) return;
    const names = await hatNames();
    if (!names.length) return;

    const missing = [];
    for (const name of names) {
      if (!(await hasProfile(name))) missing.push(name);
    }
    if (!missing.length) return;

    const copy = 'Copy `hats apply`';
    const never = "Don't show again";
    const answer = await vscode.window.showInformationMessage(
      `No VS Code profile yet for ${missing.join(', ')}. hats creates profiles while VS Code is closed: quit VS Code and run \`hats apply\`, or open a folder with \`code\` from a shell wearing the hat.`,
      copy,
      never,
    );
    if (answer === copy) await vscode.env.clipboard.writeText('hats apply');
    else if (answer === never) await context.globalState.update(SKIP_MISSING, true);
  } catch (e) {
    log.appendLine(`checkProfilesExist: ${e}`);
  }
}

/** Re-check when hats are added or removed. */
function watchHatsConfig(context) {
  try {
    const dir = vscode.Uri.file(path.join(os.homedir(), '.hats'));
    const watcher = vscode.workspace.createFileSystemWatcher(
      new vscode.RelativePattern(dir, 'config.yaml'),
    );
    const again = () => checkProfilesExist(context);
    watcher.onDidChange(again);
    watcher.onDidCreate(again);
    context.subscriptions.push(watcher);
  } catch (e) {
    log.appendLine(`watchHatsConfig: ${e}`);
  }
}

async function hatNames() {
  const out = await hats_('hat', 'list', '--plain');
  return out === undefined ? [] : out.split('\n').map((s) => s.trim()).filter(Boolean);
}

async function hasProfile(name) {
  return (await hats_('hat', 'vscode-profile', name)) !== undefined;
}

/** Run `hats`, or give up quietly: it may not be installed or on PATH. */
function hats_(...args) {
  return new Promise((resolve) => {
    execFile('hats', args, { timeout: 5000 }, (err, stdout) => {
      if (err) {
        if (err.code === 'ENOENT') log.appendLine('hats is not on PATH');
        resolve(undefined);
      } else {
        resolve(stdout);
      }
    });
  });
}

function deactivate() {}

module.exports = { activate, deactivate };
