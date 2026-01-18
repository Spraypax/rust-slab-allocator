#![no_main]

use libfuzzer_sys::fuzz_target;
use core::alloc::Layout;

use allocator::SlabAllocator;

// On active le provider std via feature
use allocator::page_provider::TestPageProvider;

const SIZE_CLASSES: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048];

#[derive(Clone, Copy)]
struct Live {
    ptr: *mut u8,
    layout: Layout,
}

fuzz_target!(|data: &[u8]| {
    // Provider std (pages allouées via alloc(Layout))
    let provider = TestPageProvider::new();
    let mut a = SlabAllocator::new(provider);

    // Table d'allocs vivantes (pour free valide)
    let mut live: [Option<Live>; 128] = [None; 128];

    let mut i = 0usize;
    while i < data.len() {
        let op = data[i] % 3; // 0 alloc, 1 free, 2 realloc-like
        i += 1;

        // index dans le tableau live
        if i >= data.len() { break; }
        let slot = (data[i] as usize) % live.len();
        i += 1;

        match op {
            // ALLOC
            0 => {
                // si slot déjà pris, on skip
                if live[slot].is_some() { continue; }

                if i >= data.len() { break; }
                let sz_idx = (data[i] as usize) % SIZE_CLASSES.len();
                i += 1;

                // align: 8/16/32/64 
                if i >= data.len() { break; }
                let align_pow = (data[i] % 4) + 3; // 3..6 => 8..64
                i += 1;
                let align = 1usize << (align_pow as usize);

                let size = SIZE_CLASSES[sz_idx];
                if align > size {
                    continue;
                }

                let layout = match Layout::from_size_align(size, align) {
                    Ok(l) => l,
                    Err(_) => continue,
                };

                let p = a.alloc(layout);
                if !p.is_null() {
                    live[slot] = Some(Live { ptr: p, layout });
                    unsafe {
                        // Petit write pour faire bouger miri/asan-like, sans dépasser
                        core::ptr::write_bytes(p, 0xA5, size.min(16));
                    }
                }
            }

            // FREE
            1 => {
                if let Some(l) = live[slot].take() {
                    unsafe { a.dealloc(l.ptr, l.layout) };
                }
            }

            // REALLOC-LIKE (free + alloc autre size)
            _ => {
                if let Some(l) = live[slot].take() {
                    unsafe { a.dealloc(l.ptr, l.layout) };
                }

                if i >= data.len() { break; }
                let sz_idx = (data[i] as usize) % SIZE_CLASSES.len();
                i += 1;

                let size = SIZE_CLASSES[sz_idx];
                let layout = match Layout::from_size_align(size, 8) {
                    Ok(l) => l,
                    Err(_) => continue,
                };

                let p = a.alloc(layout);
                if !p.is_null() {
                    live[slot] = Some(Live { ptr: p, layout });
                }
            }
        }
    }

    // Cleanup final: free tout ce qui reste
    for e in live.iter_mut() {
        if let Some(l) = e.take() {
            unsafe { a.dealloc(l.ptr, l.layout) };
        }
    }
});
