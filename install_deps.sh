#!/bin/bash

if [ ! -d "./ironfish" ];then
    git clone -b v0.1.59  https://github.com/iron-fish/ironfish.git --depth 1
    echo "deps installed(ironfish v0.1.47)."
fi
