#!/bin/bash

# Send SIGTERM to all panes in the session (more graceful shutdown)
tmux kill-session -t whitenoise-dev

echo "whitenoise-dev development environment stopped"
