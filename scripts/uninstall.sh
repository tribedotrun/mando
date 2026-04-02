#!/bin/bash
# Uninstall Mando — removes the app, data, launchd services, and logs.
# Safe to run multiple times (idempotent).

set -euo pipefail

DAEMON_LABEL="run.tribe.mando.daemon"
TG_LABEL="run.tribe.mando.telegram"

DAEMON_PLIST="$HOME/Library/LaunchAgents/$DAEMON_LABEL.plist"
TG_PLIST="$HOME/Library/LaunchAgents/$TG_LABEL.plist"

APP_PATH="/Applications/Mando.app"
DATA_DIR="$HOME/.mando"
DEV_DATA_DIR="$HOME/.mando-dev"
LOG_DIR="$HOME/Library/Logs/Mando"
SUPPORT_DIR="$HOME/Library/Application Support/Mando"

removed=()

# --- launchd services ---

bootout_service() {
  local label="$1"
  local plist="$2"

  if launchctl list "$label" &>/dev/null; then
    local err
    if err=$(launchctl bootout "gui/$(id -u)/$label" 2>&1); then
      echo "Stopped launchd service: $label"
      removed+=("launchd:$label")
    else
      case "$err" in
        *"No such process"*|*"not loaded"*|*"could not find service"*)
          echo "Stopped launchd service: $label (already stopped)"
          removed+=("launchd:$label")
          ;;
        *)
          echo "WARNING: Failed to stop $label: $err"
          echo "  The service may still be running. Kill manually: launchctl bootout gui/$(id -u)/$label"
          ;;
      esac
    fi
  fi

  if [ -f "$plist" ]; then
    rm "$plist"
    echo "Removed plist: $plist"
    removed+=("$plist")
  fi
}

bootout_service "$DAEMON_LABEL" "$DAEMON_PLIST"
bootout_service "$TG_LABEL" "$TG_PLIST"

# --- app bundle ---

if [ -d "$APP_PATH" ]; then
  rm -rf "$APP_PATH"
  echo "Removed app: $APP_PATH"
  removed+=("$APP_PATH")
fi

# --- data directory ---

if [ -d "$DATA_DIR" ]; then
  rm -rf "$DATA_DIR"
  echo "Removed data: $DATA_DIR"
  removed+=("$DATA_DIR")
fi

# --- dev data directory ---

if [ -d "$DEV_DATA_DIR" ]; then
  rm -rf "$DEV_DATA_DIR"
  echo "Removed dev data: $DEV_DATA_DIR"
  removed+=("$DEV_DATA_DIR")
fi

# --- logs ---

if [ -d "$LOG_DIR" ]; then
  rm -rf "$LOG_DIR"
  echo "Removed logs: $LOG_DIR"
  removed+=("$LOG_DIR")
fi

# --- application support ---

if [ -d "$SUPPORT_DIR" ]; then
  rm -rf "$SUPPORT_DIR"
  echo "Removed support: $SUPPORT_DIR"
  removed+=("$SUPPORT_DIR")
fi

# --- summary ---

echo ""
if [ ${#removed[@]} -eq 0 ]; then
  echo "Nothing to remove — Mando is not installed."
else
  echo "Uninstall complete. Removed ${#removed[@]} item(s)."
fi
