#!/bin/bash

export RUST_LOG=debug

# change config
pool=36.189.234.195:60006
#pool=127.0.0.1:8181
address="0df0b3222e32593d2f3103552326de56f5423ad4ba8064cbf4520ef2510a461426f10383d1c420ba73be3e"
worker_name=`hostname`
batch_size=10000
cpus=`cat /proc/cpuinfo| grep "processor"| wc -l`
echo "running zkwork_ironminer with pool($pool) address($address) worker_name($worker_name) batch($batch_size) threads{$cpus}"

./zkwork_ironminer --pool $pool  --address $address --worker_name $worker_name --batch_size $batch_size --threads $cpus
