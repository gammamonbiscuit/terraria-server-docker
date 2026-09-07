#!/bin/bash

./create-server-config.sh;

rm -rfv /var/log/terraria
./twall &
./logging.sh &
mono --server --gc=sgen -O=all ./TerrariaServer.exe -config server-config.conf -logfile /var/log/terraria
