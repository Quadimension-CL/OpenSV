use crate::synthesis::voice::{PreparedTree, Result, VoiceError};

const STREAMING_ENCODER: [u8; 16] = [
    0xe5, 0xac, 0xe7, 0x39, 0x6c, 0x2c, 0x56, 0x26, 0x6b, 0x76, 0xca, 0x93, 0xb6, 0x54, 0x62, 0xcc,
];
const DENSE_ENCODER: [u8; 16] = [
    0x46, 0xd3, 0x17, 0x2c, 0x00, 0xf5, 0x24, 0x7e, 0xfa, 0x34, 0x42, 0x3a, 0x9a, 0x98, 0x99, 0xfa,
];

pub(crate) fn encode(
    tree: &PreparedTree,
    node: usize,
    input: &[f32],
    features: usize,
) -> Result<Vec<f32>> {
    if features == 0 || !input.len().is_multiple_of(features) {
        return Err(VoiceError::new(
            "Context input does not contain complete frames",
        ));
    }
    let module = tree.node(node)?;
    let payload = tree.payload(node)?;
    let flag = match payload.as_ref() {
        [] => true,
        [value] => *value != 0,
        _ => return Err(VoiceError::new("Invalid context block configuration")),
    };
    match module.type_id {
        STREAMING_ENCODER => {
            let [attention, feed_forward, first_norm, second_norm] = module.children.as_slice()
            else {
                return Err(VoiceError::new(
                    "Streaming context block requires four children",
                ));
            };

            let attended = super::attention::execute(tree, *attention, input, features, flag)?;
            let residual = add(input, &attended)?;
            let hidden = tree.execute_node(*first_norm, &residual)?;
            let transformed = tree.execute_node(*feed_forward, &hidden)?;
            let residual = add(&hidden, &transformed)?;
            tree.execute_node(*second_norm, &residual)
        }
        DENSE_ENCODER => {
            let [
                attention,
                linear_in,
                linear_out,
                first_norm,
                second_norm,
                activation,
            ] = module.children.as_slice()
            else {
                return Err(VoiceError::new("Dense context block requires six children"));
            };

            if flag {
                return Err(VoiceError::new(
                    "Pre-normalized dense context is not recovered",
                ));
            }
            let attended = tree.execute_node(*attention, input)?;
            let residual = add(input, &attended)?;
            let hidden = tree.execute_node(*first_norm, &residual)?;

            let projected = tree.execute_node(*linear_in, &hidden)?;
            let activated = tree.execute_node(*activation, &projected)?;
            let transformed = tree.execute_node(*linear_out, &activated)?;
            let residual = add(&hidden, &transformed)?;
            tree.execute_node(*second_norm, &residual)
        }
        _ => Err(VoiceError::new("Unsupported context block type")),
    }
}

fn add(left: &[f32], right: &[f32]) -> Result<Vec<f32>> {
    if left.len() != right.len() {
        return Err(VoiceError::new("Context residual shapes do not match"));
    }
    Ok(left
        .iter()
        .zip(right)
        .map(|(left, right)| left + right)
        .collect())
}
