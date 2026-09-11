#!/bin/bash

./create-server-config.sh;

rm -rfv /var/log/terraria
./twall &
./logging.sh &
exec ./TerrariaServer.bin.x86_64 -config server-config.conf -logfile /var/log/terraria
