#!/bin/sh
# Runs ONCE after first apply. Power-user macOS preferences. Idempotent (defaults write is upsert).
# Review before enabling anything you disagree with — comment lines out to skip.
#
# PRE-REQ: grant your terminal app Full Disk Access
#   (System Settings → Privacy & Security → Full Disk Access), then fully quit + reopen it.
#   Without it, some writes (com.apple.desktopservices, sandboxed apps) fail SILENTLY.
# No sudo needed — these target the current user's domains. Do NOT run this under sudo.
set -eu

# macOS-only. On Linux (dev container) there's no `defaults` — skip cleanly.
[ "$(uname -s)" = "Darwin" ] || { echo "==> not macOS; skipping defaults."; exit 0; }

echo "==> Applying macOS defaults"

# ── Dock ──────────────────────────────────────────────────────────────────────
defaults write com.apple.dock autohide -bool true
defaults write com.apple.dock autohide-delay -float 0
defaults write com.apple.dock show-recents -bool false            # hide suggested/recent apps
defaults write com.apple.dock tilesize -int 48                    # icon size
defaults write com.apple.dock mineffect -string "scale"           # lighter minimize than genie
defaults write com.apple.dock minimize-to-application -bool true  # minimize into the app icon
defaults write com.apple.dock mru-spaces -bool false              # don't auto-rearrange Spaces
defaults write com.apple.dock launchanim -bool false              # no bounce-open animation
# The actual pinned app set is rebuilt by run_onchange_after_45-dock (via dockutil).

# ── Finder ────────────────────────────────────────────────────────────────────
defaults write NSGlobalDomain AppleShowAllExtensions -bool true              # show all file extensions
defaults write com.apple.finder ShowPathbar -bool true
defaults write com.apple.finder ShowStatusBar -bool true
defaults write com.apple.finder _FXSortFoldersFirst -bool true              # folders on top
defaults write com.apple.finder FXPreferredViewStyle -string "Nlsv"         # list view
defaults write com.apple.finder _FXShowPosixPathInTitle -bool true          # full POSIX path in title
defaults write com.apple.finder FXEnableExtensionChangeWarning -bool false  # no nag on extension change
defaults write com.apple.finder NewWindowTarget -string "PfHm"              # new windows → Home
defaults write com.apple.desktopservices DSDontWriteNetworkStores -bool true  # no .DS_Store on network shares
defaults write com.apple.desktopservices DSDontWriteUSBStores -bool true       # no .DS_Store on USB
# Show hidden files always? Uncomment (toggleable with cmd-shift-. anyway):
# defaults write com.apple.finder AppleShowAllFiles -bool true

# ── Save / print panels + window UI ───────────────────────────────────────────
defaults write NSGlobalDomain NSNavPanelExpandedStateForSaveMode -bool true
defaults write NSGlobalDomain NSNavPanelExpandedStateForSaveMode2 -bool true
defaults write NSGlobalDomain PMPrintingExpandedStateForPrint -bool true
defaults write NSGlobalDomain PMPrintingExpandedStateForPrint2 -bool true
defaults write NSGlobalDomain NSWindowResizeTime -float 0.001               # faster resize animation

# ── Keyboard: fast repeat, dev-friendly typing ────────────────────────────────
defaults write NSGlobalDomain KeyRepeat -int 2
defaults write NSGlobalDomain InitialKeyRepeat -int 15
defaults write NSGlobalDomain ApplePressAndHoldEnabled -bool false          # key repeat over accent popover
defaults write NSGlobalDomain NSAutomaticQuoteSubstitutionEnabled -bool false     # smart quotes off
defaults write NSGlobalDomain NSAutomaticDashSubstitutionEnabled -bool false      # smart dashes off
defaults write NSGlobalDomain NSAutomaticSpellingCorrectionEnabled -bool false    # autocorrect off
defaults write NSGlobalDomain NSAutomaticCapitalizationEnabled -bool false        # auto-capitalize off
defaults write NSGlobalDomain NSAutomaticPeriodSubstitutionEnabled -bool false    # double-space period off

# ── Keyboard layout: British – PC (layout id 250, com.apple.keylayout.British-PC) ─
# Sets the enabled + selected + active input source so a fresh Mac uses British PC.
# HIToolbox input-source arrays are format-sensitive; keep the plist shape exactly as below.
# Takes effect on next login (or re-login of the loginwindow); may not switch live.
defaults write com.apple.HIToolbox AppleEnabledInputSources -array \
  '{ InputSourceKind = "Keyboard Layout"; "KeyboardLayout ID" = 250; "KeyboardLayout Name" = "British-PC"; }' \
  '{ "Bundle ID" = "com.apple.PressAndHold"; InputSourceKind = "Non Keyboard Input Method"; }'
defaults write com.apple.HIToolbox AppleSelectedInputSources -array \
  '{ InputSourceKind = "Keyboard Layout"; "KeyboardLayout ID" = 250; "KeyboardLayout Name" = "British-PC"; }'
defaults write com.apple.HIToolbox AppleCurrentKeyboardLayoutInputSourceID -string "com.apple.keylayout.British-PC"

# ── Trackpad: tap to click (write both domains + currentHost for reliability) ─
defaults write com.apple.driver.AppleBluetoothMultitouch.trackpad Clicking -bool true
defaults write com.apple.AppleMultitouchTrackpad Clicking -bool true
defaults -currentHost write NSGlobalDomain com.apple.mouse.tapBehavior -int 1

# ── Menu bar ──────────────────────────────────────────────────────────────────
defaults write com.apple.controlcenter BatteryShowPercentage -bool true     # battery % (modern key)

# ── Screenshots: PNG, to ~/Screenshots, no drop shadow ────────────────────────
mkdir -p "${HOME}/Screenshots"
defaults write com.apple.screencapture location -string "${HOME}/Screenshots"
defaults write com.apple.screencapture type -string "png"
defaults write com.apple.screencapture disable-shadow -bool true

# ── RISKY — opt-in only, leave commented ──────────────────────────────────────
# Removes the "downloaded from the internet — are you sure?" first-launch warning for EVERYTHING.
# Does NOT disable Gatekeeper/XProtect/notarization, but you lose a real social-engineering defense.
# Prefer `xattr -d com.apple.quarantine <app>` per app instead.
# defaults write com.apple.LaunchServices LSQuarantine -bool false
#
# NEVER enable by default — fully disables Gatekeeper (needs sudo, high risk):
# sudo spctl --global-disable

# ── Apply: restart affected agents once (not per-write) ───────────────────────
killall Finder Dock SystemUIServer 2>/dev/null || true
echo "==> macOS defaults applied. Some global/typing keys need a logout to fully take effect."
