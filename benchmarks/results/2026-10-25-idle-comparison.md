# Performance comparison

Step 3: preserve the displayed sample while asynchronously suspending source workers and trimming unleased GPU resources and driver caches. Same final candidate binary, two 4K repetitions per mode; trim_paused intentionally differs.

Configuration changes: {"trim_paused": {"after": true, "before": false}}

| Workload | Output | CPU % | Private MiB | GPU % | Uncapped fps |
| --- | --- | ---: | ---: | ---: | ---: |
| gojo-composition | 3840x2160 | 0.04 → 0.11 | 381.41 → 382.88 | 10.39 → 10.58 | 316.28 → 323.29 |
| yuta | 3840x2160 | 0.55 → 0.47 | 736.19 → 739.89 | 3.24 → 3.31 | 591.62 → 613.36 |
| tram-video | 3840x2160 | 1.92 → 2.17 | 353.85 → 357.27 | 13.54 → 4.26 | 470.77 → 448.03 |
