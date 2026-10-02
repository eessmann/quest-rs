# QSP optimization measurements

All 72 workload records passed the campaign gates and independent summary validation. Accepted grids and export fingerprints match across all four variants and three trials.

Values below are medians of three trials. Per-invocation min/max and exact grids, fingerprints, and input receipt hashes are available in `summary.json`. Raw trial receipts remain unchanged.

## Timing

Each cell is **completion / end-to-end**, in milliseconds per invocation.

| Workload | Baseline | A | A+B | A+B+C |
|---|---:|---:|---:|---:|
| binary64 d=256, 53 bits | 0.2466 / 1.1068 | 0.1896 / 1.0342 | 0.1664 / 0.9932 | 0.1649 / 0.8940 |
| binary64 d=1024, 53 bits | 1.1183 / 5.7221 | 1.0186 / 5.3019 | 0.9294 / 4.9341 | 0.9578 / 4.7485 |
| offline d=16, 128 bits | 22.0995 / 66.6235 | 17.8775 / 62.3050 | 18.9405 / 62.7254 | 17.9326 / 61.3309 |
| offline d=16, 256 bits | 40.0394 / 106.5147 | 33.7003 / 100.1135 | 33.6065 / 100.6219 | 34.4705 / 101.1278 |
| offline d=256, 128 bits | 384.6487 / 2579.3103 | 317.4007 / 2521.8988 | 320.2736 / 2548.5965 | 319.9316 / 2494.0102 |
| offline d=256, 256 bits | 681.2280 / 4181.8110 | 595.1824 / 4152.5806 | 583.4456 / 4097.1972 | 582.3521 / 4080.3451 |

## Completion resources

Each cell is **allocations / peak additional live bytes / charged work units**.

| Workload | Baseline | A | A+B | A+B+C |
|---|---:|---:|---:|---:|
| binary64 d=256, 53 bits | 39 / 172,064 / 1,214,464 | 37 / 135,184 / 1,214,464 | 37 / 135,184 / 1,214,464 | 37 / 135,184 / 1,214,464 |
| binary64 d=1024, 53 bits | 39 / 688,160 / 5,775,360 | 37 / 540,688 / 5,775,360 | 37 / 540,688 / 5,775,360 | 37 / 540,688 / 5,775,360 |
| offline d=16, 128 bits | 228,177 / 49,544 / 446,464 | 184,982 / 37,944 / 389,120 | 184,982 / 37,944 / 389,120 | 184,982 / 37,944 / 389,120 |
| offline d=16, 256 bits | 399,204 / 102,760 / 892,928 | 332,150 / 77,256 / 778,240 | 332,150 / 77,256 / 778,240 | 332,150 / 77,256 / 778,240 |
| offline d=256, 128 bits | 4,020,884 / 772,112 / 11,337,728 | 3,329,430 / 587,712 / 9,895,936 | 3,329,430 / 587,712 / 9,895,936 | 3,329,430 / 587,712 / 9,895,936 |
| offline d=256, 256 bits | 7,159,363 / 1,620,688 / 22,675,456 | 6,077,469 / 1,215,032 / 19,791,872 | 6,077,469 / 1,215,032 / 19,791,872 | 6,077,469 / 1,215,032 / 19,791,872 |

## End-to-end resources

Each cell is **allocations / peak additional live bytes / charged work units**.

| Workload | Baseline | A | A+B | A+B+C |
|---|---:|---:|---:|---:|
| binary64 d=256, 53 bits | 8,731 / 248,856 / 8,354,304 | 8,729 / 244,744 / 8,354,304 | 8,466 / 244,744 / 7,909,888 | 7,454 / 231,104 / 7,240,192 |
| binary64 d=1024, 53 bits | 34,141 / 1,002,664 / 46,073,856 | 34,139 / 986,264 / 46,073,856 | 33,108 / 986,264 / 43,902,976 | 29,026 / 930,272 / 40,093,696 |
| offline d=16, 128 bits | 702,154 / 176,656 / 611,088 | 658,959 / 176,656 / 553,744 | 655,439 / 176,656 / 544,528 | 654,143 / 176,656 / 524,048 |
| offline d=16, 256 bits | 1,329,305 / 246,240 / 1,222,176 | 1,262,251 / 246,240 / 1,107,488 | 1,256,301 / 246,240 / 1,089,056 | 1,253,843 / 246,240 / 1,048,096 |
| offline d=256, 128 bits | 28,561,513 / 2,843,280 / 30,826,000 | 27,870,059 / 2,843,280 / 29,384,208 | 27,709,090 / 2,843,280 / 27,811,344 | 27,468,821 / 2,843,280 / 24,927,760 |
| offline d=256, 256 bits | 56,320,147 / 3,997,376 / 61,652,000 | 55,238,253 / 3,997,376 / 58,768,416 | 54,918,895 / 3,997,376 / 55,622,688 | 54,436,572 / 3,997,376 / 49,855,520 |

Allocation counts and times are normalized by the measured iteration count; peaks are sample maxima and are never divided. Counters cover Rust allocations, excluding preexisting live storage, allocator metadata, stacks, native allocation, and transient realloc overlap.

Binary64 end-to-end starts from an admitted target; offline end-to-end includes admission, export, and independent certification. Charged work follows each backend's accounting; offline solve work excludes separate certification accounting. Compare variants within a workload. Instrumentation and fixture checks are included in timings.
