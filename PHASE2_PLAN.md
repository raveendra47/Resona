# Phase 2 Implementation Plan

## Objectives
1. **Tantivy Full-Text Search** - Replace HashMap with inverted index (90-95% faster)
2. **LUFS Volume Normalization** - Add EBU R128 loudness calculation
3. **Windows File Watcher** - Native ReadDirectoryChangesW API
4. **SIMD EQ Optimization** - Leverage Intel Iris Xe SIMD capabilities

## Expected Performance Gains
- Search Response: 100-500ms → 10-20ms (90-95% faster)
- Volume Normalization: Accurate LUFS-based vs ReplayGain
- File Monitoring: Instant detection vs polling delays
- EQ Processing: 30-40% faster with SIMD

## Files to Create/Modify

### New Files:
1. `src-tauri/src/search/tantivy_index.rs` - Tantivy full-text search engine
2. `src-tauri/src/audio/lufs.rs` - LUFS loudness normalization
3. `src-tauri/src/library/windows_watcher.rs` - Native Windows file watcher
4. `src-tauri/src/audio/eq_simd.rs` - SIMD-optimized EQ processing

### Modified Files:
1. `src-tauri/Cargo.toml` - Add tantivy, loudness-rs dependencies
2. `src-tauri/src/search/mod.rs` - Export new tantivy module
3. `src-tauri/src/search/search.rs` - Replace HashMap with Tantivy
4. `src-tauri/src/library/scanner.rs` - Integrate tantivy indexing
5. `src-tauri/src/library/mod.rs` - Export windows_watcher
6. `src-tauri/src/audio/eq.rs` - Integrate SIMD optimizations
7. `src-tauri/src/audio/mod.rs` - Export lufs module
8. `src-tauri/src/commands.rs` - Update search commands
9. `src-tauri/src/main.rs` - Initialize tantivy index, lufs calculator

## Implementation Steps

### Step 1: Add Dependencies (Cargo.toml)
```toml
tantivy = "0.22"
loudness-rs = "0.1"
cpal = "0.15"  # For audio stream access
```

### Step 2: Implement Tantivy Search Index
- Create index schema with fields: title, artist, album, genre, file_path
- Implement indexing on track add/update/remove
- Implement fuzzy search with boost weights
- Replace current HashMap-based search

### Step 3: Implement LUFS Normalization
- Calculate integrated loudness using EBU R128
- Store LUFS values in database
- Apply gain adjustment during playback
- Support track/album mode switching

### Step 4: Implement Windows File Watcher
- Use ReadDirectoryChangesW for instant file change detection
- Handle create/modify/delete/rename events
- Debounce rapid changes
- Trigger library rescan on detected changes

### Step 5: Implement SIMD EQ
- Use aligned memory buffers
- Process multiple samples in parallel
- Optimize for AVX2/FMA instructions
- Benchmark against scalar implementation

### Step 6: Integration & Testing
- Update scanner to use tantivy indexing
- Update audio pipeline for LUFS normalization
- Replace notify crate with windows_watcher
- Test all features on Windows

## Success Criteria
- [ ] Search returns results in <20ms for 10k tracks
- [ ] LUFS normalization accurate within ±0.5 LU
- [ ] File changes detected within 100ms
- [ ] EQ processing uses SIMD instructions
- [ ] All tests pass on Windows
