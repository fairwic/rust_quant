use rust_quant_infrastructure::external_data::{
    DuneApiClient, DuneExecutionResponse, DuneExecutionResultsResponse, DuneExecutionState,
};
use serde_json::json;

#[test]
fn parse_execute_sql_response_extracts_execution_id_and_state() {
    let payload = json!({
        "execution_id": "01HKZJ2683PHF9Q9PHHQ8FW4Q1",
        "state": "QUERY_STATE_PENDING"
    });

    let response =
        DuneApiClient::parse_execute_sql_response(&payload).expect("execute response should parse");

    assert_eq!(
        response,
        DuneExecutionResponse {
            execution_id: "01HKZJ2683PHF9Q9PHHQ8FW4Q1".to_string(),
            state: DuneExecutionState::Pending,
        }
    );
}

#[test]
fn parse_execution_results_response_extracts_rows_and_completion_state() {
    let payload = json!({
        "execution_id": "01HKZJ2683PHF9Q9PHHQ8FW4Q1",
        "query_id": 0,
        "is_execution_finished": true,
        "state": "QUERY_STATE_COMPLETED",
        "result": {
            "metadata": {
                "column_names": ["hour_bucket", "netflow_usd"],
                "row_count": 1
            },
            "rows": [
                {
                    "hour_bucket": "2026-03-30T04:00:00Z",
                    "netflow_usd": 123456.78
                }
            ],
            "update_type": "append"
        }
    });

    let response = DuneApiClient::parse_execution_results_response(&payload)
        .expect("execution results response should parse");

    assert_eq!(
        response,
        DuneExecutionResultsResponse {
            execution_id: "01HKZJ2683PHF9Q9PHHQ8FW4Q1".to_string(),
            query_id: Some(0),
            is_execution_finished: true,
            state: DuneExecutionState::Completed,
            rows: vec![json!({
                "hour_bucket": "2026-03-30T04:00:00Z",
                "netflow_usd": 123456.78
            })],
        }
    );
}

#[test]
fn parse_execution_results_response_allows_missing_query_id_for_ad_hoc_sql() {
    let payload = json!({
        "execution_id": "01HKZJ2683PHF9Q9PHHQ8FW4Q1",
        "is_execution_finished": true,
        "state": "QUERY_STATE_COMPLETED",
        "result": {
            "metadata": {
                "column_names": ["x"],
                "row_count": 1
            },
            "rows": [
                {
                    "x": 1
                }
            ]
        }
    });

    let response = DuneApiClient::parse_execution_results_response(&payload)
        .expect("ad-hoc execution results should parse without query_id");

    assert_eq!(response.query_id, None);
    assert_eq!(response.rows, vec![json!({ "x": 1 })]);
}
