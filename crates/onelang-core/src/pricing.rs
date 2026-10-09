//! Default prices (USD per 1M tokens / characters). Users can override them per provider.

/// (model id substring, input $/1M tokens, output $/1M tokens)
const MODEL_PRICES: &[(&str, f64, f64)] = &[
    ("gpt-oss-120b", 0.15, 0.60),
    ("gpt-oss-20b", 0.075, 0.30),
    ("llama-3.3-70b", 0.59, 0.79),
    ("llama-3.1-8b", 0.05, 0.08),
    ("qwen3.8-27b", 0.80, 4.00),
    ("gpt-4.1-mini", 0.40, 1.60),
    ("gpt-4.1-nano", 0.10, 0.40),
    ("gpt-4o-mini", 0.15, 0.60),
];

pub fn default_model_price(model: &str) -> Option<(f64, f64)> {
    let m = model.to_ascii_lowercase();
    MODEL_PRICES
        .iter()
        .find(|(id, _, _)| m.contains(id))
        .map(|(_, i, o)| (*i, *o))
}

pub fn token_cost(input_tokens: u32, output_tokens: u32, price_in: f64, price_out: f64) -> f64 {
    (input_tokens as f64 * price_in + output_tokens as f64 * price_out) / 1_000_000.0
}

pub fn char_cost(chars: u32, price_per_million: f64) -> f64 {
    chars as f64 * price_per_million / 1_000_000.0
}
