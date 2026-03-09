# Debug: Parallel Tasks Test

A test plan with independent and dependent tasks to verify wave-based parallel execution.

## Phase 1: Independent file creation

- [ ] Create `debug/output/alpha.txt` with the content "Alpha module initialized"
- [ ] Create `debug/output/beta.txt` with the content "Beta module initialized"
- [ ] Create `debug/output/gamma.txt` with the content "Gamma module initialized"

## Phase 2: Aggregation (depends on Phase 1)

- [ ] Create `debug/output/combined.txt` that reads alpha.txt, beta.txt, and gamma.txt and combines their contents into one file, one line per source file
- [ ] Create `debug/output/manifest.txt` listing all files in `debug/output/` (one filename per line)

## Phase 3: Verification

- [ ] Verify that `debug/output/combined.txt` contains exactly 3 lines
- [ ] Verify that `debug/output/manifest.txt` lists at least 5 files (alpha, beta, gamma, combined, manifest)
