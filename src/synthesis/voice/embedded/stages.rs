// Registry identities for SV1 synthesis stages.
use super::{Entry, types};
#[rustfmt::skip]
pub(super) const ENTRIES: &[Entry] = &[
    // DURATION_TYPE_ID
    Entry { aliases: [
        0x4c30eaff390f599a, 0x210ae732bcda3950, 0xedb2643ec31049d7, 0x76b77e5bf8d6500c,
        0x8e0f0182b380a860, 0xd4c2d7946f91e167, 0x135ee08897501166, 0x71a4b344df3f7ece,
        0xa4f623b00c5a97c0, 0x1fad6bd2dfa1a17f, 0x5d5d600f0e5453e9, 0x1db558504d28a4ad,
    ], id: types::DURATION_TYPE_ID },
    // DURATION_FRONTEND_TYPE_ID
    Entry { aliases: [
        0xff369e74e595e206, 0xea62ca7c31e37ca0, 0x319b1e6e37917bd3, 0x05909feba966bb54,
        0x18b5591e22aba550, 0x825a31102572a303, 0xf0c8bdd6e4fe863a, 0x77f5f7f551d96e82,
        0xd9177b5063843fb0, 0xa097f32e2e5fe07b, 0xc6ea192cf9c095d1, 0x32870689962bff1d,
    ], id: types::DURATION_FRONTEND_TYPE_ID },
    // F0_WRAPPER_TYPE_ID
    Entry { aliases: [
        0xcce2240f4753c837, 0x0bc3c8e11c368859, 0xb7d76850e579425a, 0xa81b4e0a342431ed,
        0xfd1c9123b5ccf269, 0x736a014c0a00406a, 0x933cd542e8c3839b, 0xdb0be7fe929c02e3,
        0xbbcb3355f5331ec9, 0xfdf99c04dc496ac2, 0x8cd901ed074da870, 0x3b69b825e5c1bb3c,
    ], id: types::F0_WRAPPER_TYPE_ID },
    // F0_ROOT_TYPE_ID
    Entry { aliases: [
        0x5b8562d18c978b91, 0xe781be473240733f, 0xa8ad7b2715fc5e90, 0xef133a930d94f0fb,
        0x0461232f81e7260f, 0x6715e41310bb7ce0, 0xbcd2df869cfd154d, 0xa7dc6559d4dcf415,
        0xc310c561c14f056f, 0xc58a4d93dd623a98, 0xe32797f2732f379a, 0x1bd8f79c0eaca55e,
    ], id: types::F0_ROOT_TYPE_ID },
    // F0_FRONTEND_TYPE_ID
    Entry { aliases: [
        0x326912e58397973c, 0x41cc3ee418da5076, 0xeb267c0afe5fcfad, 0x42950c54a390f95a,
        0x9d63b0bd15221986, 0x13129a19647e31bd, 0x86149a203f186a10, 0x1051cbcb9d995bb8,
        0x5dc552ef55f9da66, 0x7430ff6f1646a6f5, 0xf35e005c07d8d7e3, 0xc6df464d8865892f,
    ], id: types::F0_FRONTEND_TYPE_ID },
    // F0_FEATURES_TYPE_ID
    Entry { aliases: [
        0xa5e321251caa33ed, 0xef76985afa86ddf3, 0xad6ece198010cb64, 0x2b04774298206d1f,
        0xf396d3b2a8582243, 0x01771a1f6fab6574, 0xb1ec6d6128b14ae9, 0xfa28a01cd2e6d271,
        0xb092f5e4e64d93a3, 0x470db6b41577b68c, 0x8ef5b03449c54d0e, 0x3ff67ea05f320122,
    ], id: types::F0_FEATURES_TYPE_ID },
    // F0_SAMPLER_TYPE_ID
    Entry { aliases: [
        0x67133569b1e0c9a7, 0x0833163a9cfbe985, 0x9d1ceeb019e4ee82, 0xc2d683aaffb9c539,
        0x7878dd761d208ab5, 0x833c88b73fdaad72, 0x83853dd7b2ffc31b, 0xcd0750935e49d6e3,
        0x37277fa85c86b715, 0x0dcc23701223d7ca, 0xe886cc8e9fe491d4, 0xca7b4a8a02707df0,
    ], id: types::F0_SAMPLER_TYPE_ID },
    // VOCODER_TYPE_ID
    Entry { aliases: [
        0x6dfb67909b0414a1, 0xbc0acd420e51b743, 0x1fd1749826b0caec, 0x88f2759ebd085477,
        0x9b000cb774e2a953, 0x44d4cfabea5dd27c, 0x7963f08abeb98d85, 0x950e9aca274ee27d,
        0xa919046c636db7b3, 0x7e6dcc7c7f9f8224, 0x9bc6670466293322, 0xb244cc8626a05cae,
    ], id: types::VOCODER_TYPE_ID },
    // TIMBRE_TYPE_ID
    Entry { aliases: [
        0x65f83b630b2ee1b8, 0x90b629148ea62baa, 0xf8b7e5f2247aea91, 0xd20259c0e806fa2e,
        0x7bd3c46b9c3732fa, 0x625bd1f8b170e5a1, 0x32d43b4d6aafabcc, 0xc5c47d700e602964,
        0x3c36669ddd10a6da, 0x355455a74d9cb749, 0xa955720c55a459b7, 0x73d6e15df7e36503,
    ], id: types::TIMBRE_TYPE_ID },
    // TIMBRE_SAMPLER_TYPE_ID
    Entry { aliases: [
        0x16a84462dde06b5f, 0xb7f9921413d5e0b9, 0x446b5cf24eb7cefa, 0x82e8e2c0bae6ecc5,
        0xb8b34d6bbec32289, 0x987328f83e518f8a, 0x59ab444cef839edb, 0x8c6296707eb3dc83,
        0x7915ef9dff9c9669, 0xf7b6bca72a6f8382, 0xe29ca90be53a66d0, 0x361dd85dd49eab34,
    ], id: types::TIMBRE_SAMPLER_TYPE_ID },
    // TIMBRE_FRONTEND_TYPE_ID
    Entry { aliases: [
        0x1b5eed43104ea4df, 0x949cb4b81b0a1c59, 0x67e269bcaa003b62, 0x46dce280e46a86f5,
        0x6cae04625dd22429, 0xe483ae2b0679b6b2, 0xc55edd8b874ff2fb, 0xb3df18158fe759b3,
        0x2d1026949eaabe89, 0x1c1470e48cbf6cba, 0x7804797c05f01b98, 0xda18d7aed590cc7c,
    ], id: types::TIMBRE_FRONTEND_TYPE_ID },
    // TIMBRE_FRONTEND_V2_TYPE_ID
    Entry { aliases: [
        0x1b5eec43104ea32c, 0x949cb3b81b0a1aa6, 0x67e26abcaa003d15, 0x46dce180e46a8542,
        0x6cae03625dd22276, 0xe483af2b0679b865, 0xc55edc8b874ff148, 0xb3df17158fe75800,
        0x2d1025949eaabcd6, 0x1c1471e48cbf6e6d, 0x78047a7c05f01d4b, 0xda18d8aed590ce2f,
    ], id: types::TIMBRE_FRONTEND_V2_TYPE_ID },
    // TIMBRE_CONTEXT_TYPE_ID
    Entry { aliases: [
        0x09deb3c2f9e17ee8, 0x4260f00134be367a, 0x5b5095af3bfad7b9, 0x7160e45df511f95e,
        0x9f1a9d3c06554a4a, 0xd82bb648b3580089, 0x0959b57428a2267c, 0x09b6e6046eb6adf4,
        0x5f7d3f6e472ebe2a, 0x0ae952dcbe211531, 0x0ef89b00ef901cc7, 0x565d55b357940a53,
    ], id: types::TIMBRE_CONTEXT_TYPE_ID },
    // TIMBRE_DIFFUSION_TYPE_ID
    Entry { aliases: [
        0x7ff6c6277010825d, 0x26f7bf69219d4cf7, 0xf3d7de951b2bd7d0, 0x4928f54e2d5665d3,
        0x2a80859e249766c7, 0xc725b790dedbfa20, 0x0f7e869e2dc27479, 0x4c5648c5b9cd9571,
        0x416827cb7d722fa7, 0xb39f42d8e751b988, 0x5397330e2de6217e, 0x88549a0dbd042e62,
    ], id: types::TIMBRE_DIFFUSION_TYPE_ID },
    // F0_FRONTEND_EQUIVALENT
    Entry { aliases: [
        0x326915e583979c55, 0x41cc3de418da4ec3, 0xeb26790afe5fca94, 0x42950b54a390f7a7,
        0x9d63afbd152217d3, 0x13129719647e2ca4, 0x86149d203f186f29, 0x1051cecb9d9960d1,
        0x5dc551ef55f9d8b3, 0x7430fc6f1646a1dc, 0xf35e015c07d8d996, 0xc6df474d88658ae2,
    ], id: types::F0_FRONTEND_TYPE_ID },
    // prim2
    Entry { aliases: [
        0x2b189d52a7d5284a, 0x580e7f261fbd33f8, 0x884331b14da58263, 0x0d36999c8a2a832c,
        0xf903d9ccdf4f9068, 0xd902e12b4a80aa93, 0xea67b5217473a52e, 0x585cad7e8c2e51e6,
        0x229d834f28a49188, 0xce8a79eed68f751b, 0x618fd1bc101c6bed, 0xff332efe8be62389,
    ], id: [0x53, 0x56, 0x31, 0x54, 0x59, 0x50, 0x45, 0x00, 0x4a, 0x28, 0xd5, 0xa7, 0x52, 0x9d, 0x18, 0x2b] },
];
