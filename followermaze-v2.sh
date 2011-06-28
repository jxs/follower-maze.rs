#! /bin/bash

SEED=666
EVENTS=100
CONCURRENCY=7

time java -server -Xmx1G -jar ./follower-maze-2.0.jar $SEED $EVENTS $CONCURRENCY
