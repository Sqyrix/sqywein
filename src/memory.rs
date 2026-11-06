//! Görev Yöneticisi'nin "Bellek" sütunuyla aynı ölçü: özel çalışma kümesi.
#![allow(dead_code)] // RAM göstergesi ileride başka bir yere taşınacak.

use windows::Win32::{
    Foundation::CloseHandle,
    System::{
        ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    },
};

/// PROCESS_MEMORY_COUNTERS_EX2: PrivateWorkingSetSize alanı yalnızca burada var.
#[repr(C)]
#[derive(Default)]
struct CountersEx2 {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
    private_usage: usize,
    private_working_set_size: usize,
    shared_commit_usage: u64,
}

pub fn private_bytes(pid: u32) -> u64 {
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return 0; // Süreç bu arada kapanmış olabilir.
        };
        let mut c = CountersEx2 { cb: std::mem::size_of::<CountersEx2>() as u32, ..Default::default() };
        let ok = K32GetProcessMemoryInfo(handle, &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb).as_bool();
        let _ = CloseHandle(handle);
        if ok { c.private_working_set_size as u64 } else { 0 }
    }
}

pub fn total(pids: impl IntoIterator<Item = u32>) -> u64 {
    pids.into_iter().map(private_bytes).sum()
}

pub fn format_mb(bytes: u64) -> String {
    format!("{} MB", (bytes + 512 * 1024) / (1024 * 1024))
}

#[cfg(test)]
mod tests {
    #[test]
    fn kendi_bellegi() {
        let pid = std::process::id();
        let mb = super::private_bytes(pid) / (1024 * 1024);
        // Derleyici diziyi "kullanılmıyor" diye atmasın: her sayfaya gerçekten yazılır.
        let mut v: Vec<u8> = vec![0; 64 * 1024 * 1024];
        for i in (0..v.len()).step_by(4096) {
            v[i] = 1;
        }
        let v = std::hint::black_box(v);
        let after = super::private_bytes(pid) / (1024 * 1024);
        println!("önce {mb} MB, 64 MB ayırınca {after} MB ({})", v.len());
        assert!(after >= mb + 50, "özel bellek okunamıyor");
    }
}
