#!/bin/bash

# Function to check and setup ADB port forwarding
setup_adb_ports() {
    echo "Checking ADB port forwarding..."

    # Check if ports are already forwarded
    local port_list=$(adb reverse --list)
    local need_7777=$(echo "$port_list" | grep -c "7777")
    local need_8080=$(echo "$port_list" | grep -c "8080")

    # Setup port 7777 if not forwarded
    if [ $need_7777 -eq 0 ]; then
        echo "Setting up port forwarding for 7777..."
        adb reverse tcp:7777 tcp:7777
    fi

    # Setup port 8080 if not forwarded
    if [ $need_8080 -eq 0 ]; then
        echo "Setting up port forwarding for 8080..."
        adb reverse tcp:8080 tcp:8080
    fi
}

# Run port setup
setup_adb_ports

# Start a new tmux session named 'whitenoise-dev' if it doesn't exist
tmux new-session -d -s whitenoise-dev

# Split the window vertically into three equal panes
tmux split-window -v -p 66
tmux split-window -v -p 50

# Send commands to each pane
# Top pane - just dev-and
tmux send-keys -t whitenoise-dev:0.0 'just dev-and' C-m

# Middle pane - just log-and
tmux send-keys -t whitenoise-dev:0.1 'just log-and' C-m

# Bottom pane - docker compose up
tmux send-keys -t whitenoise-dev:0.2 'docker compose up' C-m

# Select the first pane
tmux select-pane -t whitenoise-dev:0.0

# Make all panes equal size
tmux select-layout even-vertical

# Attach to the session
tmux attach-session -t whitenoise-dev
