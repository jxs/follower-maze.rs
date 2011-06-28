#! /bin/bash

SEED=666
EVENTS=1000
CONCURRENCY=7

time java -server -XX:-UseConcMarkSweepGC -Xmx2G -jar ./FollowerMaze-assembly-1.0.jar $SEED $EVENTS $CONCURRENCY
