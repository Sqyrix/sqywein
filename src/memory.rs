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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pressure { #[default] Normal, Elevated, Critical }

impl Pressure {
    /// Fiziksel RAM ve commit alanı; histerezis eşik yakınında gidip gelmeyi önler.
    pub fn next(self, total: u64, available: u64, commit_total: u64, commit_free: u64) -> Self {
        if total == 0 { return self; }
        let mb = 1024 * 1024;
        let critical = available < (total / 20).max(512 * mb)
            || (commit_total > 0 && commit_free < commit_total / 20);
        let elevated = available < (total * 12 / 100).max(1024 * mb)
            || (commit_total > 0 && commit_free < commit_total / 10);
        let recovered = available >= (total * 18 / 100).max(1536 * mb)
            && (commit_total == 0 || commit_free >= commit_total * 15 / 100);
        if critical || (self == Self::Critical && (available < (total * 8 / 100).max(768 * mb)
            || (commit_total > 0 && commit_free < commit_total * 8 / 100))) { Self::Critical }
        else if elevated || (self != Self::Normal && !recovered) { Self::Elevated }
        else { Self::Normal }
    }

    pub fn sample(self) -> Self {
        use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut status = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        if unsafe { GlobalMemoryStatusEx(&mut status) }.is_err() { return self; }
        self.next(status.ullTotalPhys, status.ullAvailPhys, status.ullTotalPageFile, status.ullAvailPageFile)
    }

    /// Dondurma ve yalnızca baskı altında otomatik boşaltma eşikleri (saniye).
    pub fn delays(self) -> (u64, Option<u64>) {
        match self { Self::Normal => (30, None), Self::Elevated => (15, Some(180)), Self::Critical => (10, Some(60)) }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn pressure_recovers_without_thrashing_and_handles_commit_exhaustion() {
        use super::Pressure::*;
        let gb = 1024 * 1024 * 1024;
        assert_eq!(Normal.next(16*gb, 8*gb, 24*gb, 12*gb), Normal);
        assert_eq!(Normal.next(16*gb, gb, 24*gb, 12*gb), Elevated);
        assert_eq!(Normal.next(16*gb, gb/2, 24*gb, 12*gb), Critical);
        assert_eq!(Critical.next(16*gb, gb, 24*gb, 12*gb), Critical);
        assert_eq!(Critical.next(16*gb, 2*gb, 24*gb, 12*gb), Elevated);
        assert_eq!(Elevated.next(16*gb, 3*gb, 24*gb, 12*gb), Normal);
        assert_eq!(Normal.next(16*gb, 8*gb, 24*gb, gb/2), Critical);
        assert_eq!(Normal.delays().1, None);
        assert_eq!(Critical.delays().1, Some(60));
    }
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
