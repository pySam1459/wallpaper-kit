# Performance snapshot after optimization

Release hardware runs. Values are medians across repetitions; JSON also retains the minimum and maximum.
CPU % uses total logical CPU capacity. Private MiB is process-tree committed memory during paced playback. GPU % is the busiest owned engine.

| Workload | Output | Paced fps | Uncapped fps | CPU % | Private MiB | GPU % | p95 gap ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| blank | 1920x1080 | 60.00 | 650.45 | 0.03 | 63.55 | 2.06 | 17.68 |
| static | 1920x1080 | 60.00 | 633.07 | 0.16 | 73.48 | 2.59 | 17.83 |
| panda | 1920x1080 | 60.00 | 621.54 | 0.04 | 315.78 | 6.24 | 18.12 |
| gojo-particles | 1920x1080 | 60.00 | 607.05 | 0.17 | 297.05 | 4.56 | 17.73 |
| gojo-composition | 1920x1080 | 60.00 | 632.39 | 0.12 | 312.97 | 4.70 | 17.23 |
| yuta | 1920x1080 | 60.00 | 608.72 | 0.45 | 737.79 | 5.27 | 19.18 |
| tram-video | 1920x1080 | 60.00 | 469.17 | 2.21 | 354.35 | 8.01 | 18.88 |
| blank | 3840x2160 | 60.00 | 631.97 | 0.05 | 64.67 | 1.97 | 17.23 |
| static | 3840x2160 | 60.00 | 637.01 | 0.12 | 74.89 | 4.09 | 18.36 |
| panda | 3840x2160 | 60.00 | 625.58 | 0.09 | 341.38 | 1.82 | 17.26 |
| gojo-particles | 3840x2160 | 60.00 | 619.04 | 0.18 | 299.29 | 3.59 | 17.42 |
| gojo-composition | 3840x2160 | 60.00 | 325.73 | 0.16 | 386.34 | 10.61 | 17.32 |
| yuta | 3840x2160 | 60.00 | 614.51 | 0.43 | 737.11 | 3.94 | 18.60 |
| tram-video | 3840x2160 | 60.00 | 444.41 | 2.15 | 359.93 | 7.08 | 18.97 |
