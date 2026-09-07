#!/bin/bash
if [[ $log == 1 || $log == true ]]; then
    until compgen -G "/var/log/terraria/Log_*.log" > /dev/null; do
        sleep 1
        echo "Waiting for log file"
    done
    tail -n +1 -f /var/log/terraria/Log_*.log>${TERRARIA_DIR}/Logs/${world%.*}.log
fi