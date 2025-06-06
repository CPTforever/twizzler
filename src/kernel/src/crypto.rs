use p256::ecdsa::{
    signature::{self, Signer, Verifier},
    Signature, SigningKey, VerifyingKey,
};
use sha2::{Digest, Sha256};

pub fn sha256(input: impl AsRef<[u8]>) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let res = hasher.finalize();
    res.into()
}

pub fn sign(private_key: &SigningKey, message: &[u8]) -> Signature {
    private_key.sign(message)
}

pub fn verify(
    public_key: &VerifyingKey,
    message: &[u8],
    signature: Signature,
) -> signature::Result<()> {
    public_key.verify(message, &signature)
}

mod test {

    use core::hint::black_box;

    use hex_literal::hex;
    use twizzler_kernel_macros::kernel_test;

    use super::*;
    use crate::utils::benchmark;

    #[kernel_test]
    fn test_hashing() {
        let expected = hex!("09ca7e4eaa6e8ae9c7d261167129184883644d07dfba7cbfbc4c8a2e08360d5b");
        let hash = sha256(b"hello, world");
        assert_eq!(hash[..], expected);
    }

    #[kernel_test]
    fn bench_hashing() {
        benchmark(|| {
            let hash = sha256(b"hello, world");
            black_box(hash);
        });
    }

    #[kernel_test]
    fn test_signature() {
        let key = [
            168, 182, 114, 184, 168, 191, 237, 9, 90, 139, 135, 141, 26, 180, 247, 51, 86, 17, 197,
            11, 229, 2, 25, 252, 9, 84, 135, 246, 235, 97, 11, 60,
        ];
        let private_key = SigningKey::from_slice(&key).unwrap();
        let message =
            b"ECDSA proves knowledge of a secret number in the context of a single message";
        let signature: Signature = sign(&private_key, message);

        let pub_key: VerifyingKey = private_key.into();
        verify(&pub_key, message, signature).expect("should be a valid signature");
    }

    #[kernel_test]
    fn bench_signing() {
        let key = [
            168, 182, 114, 184, 168, 191, 237, 9, 90, 139, 135, 141, 26, 180, 247, 51, 86, 17, 197,
            11, 229, 2, 25, 252, 9, 84, 135, 246, 235, 97, 11, 60,
        ];
        let private_key = SigningKey::from_slice(&key).unwrap();
        let message =
            b"ECDSA proves knowledge of a secret number in the context of a single message";

        benchmark(|| {
            let _signature: Signature = black_box(sign(&private_key, message));
        });
    }
    #[kernel_test]
    fn bench_verifying() {
        let key = [
            168, 182, 114, 184, 168, 191, 237, 9, 90, 139, 135, 141, 26, 180, 247, 51, 86, 17, 197,
            11, 229, 2, 25, 252, 9, 84, 135, 246, 235, 97, 11, 60,
        ];
        let private_key = SigningKey::from_slice(&key).unwrap();
        let message =
            b"ECDSA proves knowledge of a secret number in the context of a single message";
        let signature: Signature = black_box(sign(&private_key, message));
        let pub_key: VerifyingKey = private_key.into();

        benchmark(|| {
            let _ver = black_box(
                verify(&pub_key, message, signature).expect("should be a valid signature"),
            );
        });
    }
}
