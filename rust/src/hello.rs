//! Windows Hello ile kimlik doğrulama (PIN, parmak izi, yüz).
//!
//! Kayıtlı şifreleri göstermeden ya da dışa aktarmadan önce istenir; Chrome'un
//! Windows'ta yaptığı gibi. Pencere ana pencereye bağlı açılır, sonuç arka
//! plandan `Background::Hello` ile gelir.

use crate::engine::{self, Background};
use windows::{
    core::*,
    Security::Credentials::UI::{UserConsentVerificationResult, UserConsentVerifier},
    Win32::{Foundation::HWND, System::WinRT::IUserConsentVerifierInterop},
};
use windows_future::IAsyncOperation;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Verified,
    /// Bu bilgisayarda Windows Hello kurulu değil ya da kapalı.
    Unavailable,
    /// Kullanıcı vazgeçti ya da doğrulanamadı.
    Denied,
}

fn outcome(r: UserConsentVerificationResult) -> Outcome {
    match r {
        UserConsentVerificationResult::Verified => Outcome::Verified,
        UserConsentVerificationResult::DeviceNotPresent
        | UserConsentVerificationResult::NotConfiguredForUser
        | UserConsentVerificationResult::DisabledByPolicy => Outcome::Unavailable,
        _ => Outcome::Denied,
    }
}

/// Doğrulama penceresini açar; `request` sonuçla birlikte geri döner.
pub fn verify(hwnd: HWND, message: &str, request: u32) {
    let op: Result<IAsyncOperation<UserConsentVerificationResult>> = (|| unsafe {
        let interop = factory::<UserConsentVerifier, IUserConsentVerifierInterop>()?;
        interop.RequestVerificationForWindowAsync(hwnd, &HSTRING::from(message))
    })();
    match op {
        Ok(op) => {
            // Bekleme arka planda: arayüz donmaz.
            std::thread::spawn(move || {
                let result = op.join().map(outcome).unwrap_or(Outcome::Denied);
                engine::push_background(Background::Hello(request, result));
            });
        }
        Err(_) => engine::push_background(Background::Hello(request, Outcome::Unavailable)),
    }
}
