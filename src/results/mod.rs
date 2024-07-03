use crate::errors::LoadGenError;

pub async fn process_results(mut result_durations: Vec<u128>, mut result_errors: Vec<u16>) -> Result<(), LoadGenError> {
    if result_durations.is_empty() || result_errors.is_empty() {
        return Err(LoadGenError::NoResultsError);
    }
    result_durations.sort_unstable();
    result_errors.sort_unstable();

    let mut total_5xx_responses = 0;
    result_errors.iter().for_each(|value| {
        if value > &499_u16 && value < &599_u16 {
            total_5xx_responses += 1;
        }
    });
    let median = result_durations.len() / 2;
    let success_rate: f32 = ((1 - (total_5xx_responses / result_errors.len())) * 100) as f32;
    println!("success: {:.2} %", success_rate);
    let formatted_p50 = format_duration_as_seconds(result_durations[median]).await;
    println!("median: {}s", formatted_p50);
    Ok(())
}

async fn format_duration_as_seconds(duration: u128) -> f32 {
    duration as f32 / 1000f32
}