#! /bin/bash

SEED=666
export totalEvents=1000000
CONCURRENCY=7

time java -server -Xmx1G -jar ./follower-maze-2.0.jar 
