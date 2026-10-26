# Initial performance baseline

Release hardware runs. Values are medians across repetitions; JSON also retains the minimum and maximum.
CPU % uses total logical CPU capacity. Private MiB is process-tree committed memory during paced playback. GPU % is the busiest owned engine.

| Workload | Output | Paced fps | Uncapped fps | CPU % | Private MiB | GPU % | p95 gap ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| gojo-composition | 3840x2160 | 60.00 | 323.29 | 0.11 | 382.88 | 10.58 | 17.25 |
| yuta | 3840x2160 | 60.00 | 613.36 | 0.47 | 739.89 | 3.31 | 18.55 |
| tram-video | 3840x2160 | 60.00 | 448.03 | 2.17 | 357.27 | 4.26 | 18.41 |
