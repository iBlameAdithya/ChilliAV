use chilli_analytics::AnalyticsMultiAgentOrchestrator;

#[test]
fn test_bazooka_proof_security_and_unknown_handling() {
    let orchestrator = AnalyticsMultiAgentOrchestrator::new().expect("Failed to initialize orchestrator");

    // 1. SQL Injection attack attempt -> Must be cleanly rejected
    let res_sqli = orchestrator.execute_query("DROP TABLE crm_leads; --", false);
    assert!(res_sqli.is_err(), "SQL Injection query must return error");
    assert_eq!(res_sqli.unwrap_err().to_string(), "Not enough data to be processed");

    // 2. Destructive mutation attempt -> Must be cleanly rejected
    let res_del = orchestrator.execute_query("DELETE FROM sales_orders WHERE 1=1", false);
    assert!(res_del.is_err(), "Destructive mutation query must return error");

    // 3. Out-of-Domain trick question -> Must be cleanly rejected
    let res_trick = orchestrator.execute_query("Who is the president of US?", false);
    assert!(res_trick.is_err(), "Out-of-domain question must return error");

    // 4. Gibberish input -> Must be cleanly rejected
    let res_gibberish = orchestrator.execute_query("asdfghjkl 12345 !@#$%^&*()", false);
    assert!(res_gibberish.is_err(), "Gibberish query must return error");

    // 5. Valid E-Commerce trend query -> Must succeed cleanly
    let res_valid = orchestrator.execute_query("Show monthly sales trend for last year.", false);
    assert!(res_valid.is_ok(), "Valid query must succeed");

    // 6. Valid Voice filler query -> Must succeed cleanly
    let res_voice = orchestrator.execute_query("um uh show me lead status distribution please", true);
    assert!(res_voice.is_ok(), "Valid voice query must succeed");
}
