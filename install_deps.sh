#!/bin/bash

if [ ! -d "./ironfish" ];then
    git clone -b zk-0.1.47 https://github.com/6block/ironfish.git --depth 1
    echo "deps installed(ironfish v0.1.47)."
fi
