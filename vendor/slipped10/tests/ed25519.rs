use core::str::FromStr;

use hex::FromHex;
use slipped10::*;

#[test]
fn test_ed25519() {
    let test_vector = [
        (
            "000102030405060708090a0b0c0d0e0f",
            [
                (
                    "m",
                    "90046a93de5380a72b5e45010748567d5ea02bbf6522f979e05c0d8d8ca9fffb",
                    "2b4be7f19ee27bbf30c667b642d5f4aa69fd169872f8fc3059c08ebae2eb19e7",
                    "00a4b2856bfec510abab89753fac1ac0e1112364e7d250545963f135f2a33188ed"
                ),
                (
                    "m/0H/1H",
                    "a320425f77d1b5c2505a6b1b27382b37368ee640e3557c315416801243552f14",
                    "b1d0bad404bf35da785a64ca1ac54b2617211d2777696fbffaf208f746ae84f2",
                    "001932a5270f335bed617d5b935c80aedb1a35bd9fc1e31acafd5372c30f5c1187"
                ),
            ]
        ),
        (
            "fffcf9f6f3f0edeae7e4e1dedbd8d5d2cfccc9c6c3c0bdbab7b4b1aeaba8a5a29f9c999693908d8a8784817e7b7875726f6c696663605d5a5754514e4b484542",
            [
                (
                    "m",
                    "ef70a74db9c3a5af931b5fe73ed8e1a53464133654fd55e7a66f8570b8e33c3b",
                    "171cb88b1b3c1db25add599712e36245d75bc65a1a5c9e18d76f9f2b1eab4012",
                    "008fe9693f8fa62a4305a140b9764c5ee01e455963744fe18204b4fb948249308a"
                ),
                (
                    "m/0H",
                    "0b78a3226f915c082bf118f83618a618ab6dec793752624cbeb622acb562862d",
                    "1559eb2bbec5790b0c65d8693e4d0875b1747f4970ae8b650486ed7470845635",
                    "0086fab68dcb57aa196c77c5f264f215a112c22a912c10d123b0d03c3c28ef1037"
                ),
            ],
        ),
    ];

    for (seed, tests) in test_vector.iter() {
        let seed = &Vec::from_hex(seed).unwrap();
        for (chain, chain_code, private, public) in tests {
            let chain = BIP32Path::from_str(chain).unwrap();
            let key = derive_key_from_path(&seed, Curve::Ed25519, &chain).unwrap();
            assert_eq!(&key.chain_code[..], &Vec::from_hex(chain_code).unwrap()[..]);
            assert_eq!(&key.key[..], &Vec::from_hex(private).unwrap()[..]);
            assert_eq!(&key.public_key()[..], &Vec::from_hex(public).unwrap()[..]);
        }
    }
}
