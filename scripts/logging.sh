#!/bin/bash
mkdir -p ${TERRARIA_DIR}/Logs
cd ${TERRARIA_DIR}/Logs
rm -fv Log_*.log
until compgen -G "Log_*.log" > /dev/null; do
    sleep 1
    echo "Waiting for log file"
done
tail -n +1 -F Log_*.log>server.log