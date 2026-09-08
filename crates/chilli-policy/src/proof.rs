use blake3::Hasher;

/// Cryptographic Proof of Execution Generator (SOC2 / ISO 27001 Compliance)
/// Generates an immutable cryptographic hash token for verified agent execution traces
pub fn generate_execution_proof(raw_query: &str, primary_sql: &str, domain: &str, timestamp: &str) -> String {
    let mut hasher = Hasher::new();
    hasher.update(b"CHILLIAV_AUDIT_PROOF_V1:");
    hasher.update(raw_query.as_bytes());
    hasher.update(b":");
    hasher.update(primary_sql.as_bytes());
    hasher.update(b":");
    hasher.update(domain.as_bytes());
    hasher.update(b":");
    hasher.update(timestamp.as_bytes());
    format!("blake3:{}", hasher.finalize().to_hex())
}

pub fn verify_execution_proof(raw_query: &str, primary_sql: &str, domain: &str, timestamp: &str, proof: &str) -> bool {
    let expected = generate_execution_proof(raw_query, primary_sql, domain, timestamp);
    expected == proof
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cryptographic_proof_generation() {
        let proof = generate_execution_proof("Show sales", "SELECT * FROM sales_orders;", "ECommerce", "2026-09-08");
        assert!(proof.starts_with("blake3:"));
        assert!(verify_execution_proof("Show sales", "SELECT * FROM sales_orders;", "ECommerce", "2026-09-08", &proof));
        assert!(!verify_execution_proof("Tampered", "SELECT * FROM sales_orders;", "ECommerce", "2026-09-08", &proof));
    }
}
