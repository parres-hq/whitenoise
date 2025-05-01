#!/bin/bash

# Exit if tmux session already exists
if tmux has-session -t whitenoise-dev 2>/dev/null; then
    echo "Session 'whitenoise-dev' already exists. Attaching..."
    tmux attach -t whitenoise-dev
    exit 0
fi

# Create new tmux session named 'whitenoise-dev' but don't attach to it
tmux new-session -d -s whitenoise-dev

# Split the window vertically (top/bottom)
tmux split-window -v

# Select first pane (top) and run just dev
tmux select-pane -t 0
tmux send-keys 'just dev' C-m

# Select second pane (bottom) and run docker compose up
tmux select-pane -t 1
tmux send-keys 'docker compose up' C-m

# Attach to the session
tmux attach -t whitenoise-dev
