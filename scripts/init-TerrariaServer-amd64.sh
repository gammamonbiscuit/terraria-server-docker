#!/bin/bash

./create-server-config.sh;

rm -rfv /var/log/terraria
./logging.sh &
./TerrariaServer.bin.x86_64 -config server-config.conf -logfile /var/log/terraria
