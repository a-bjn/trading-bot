pub fn calculate_percentage_change(old_value: f64, new_value: f64) -> f64 {
    if old_value == 0.0 {
        return 0.0;
    }
    ((new_value - old_value) / old_value) * 100.0
}

pub fn calculate_slippage(expected: f64, actual: f64) -> f64 {
    calculate_percentage_change(expected, actual).abs()
}

pub fn sol_to_lamports(sol: f64) -> u64 {
    (sol * 1_000_000_000.0) as u64
}

pub fn lamports_to_sol(lamports: u64) -> f64 {
    lamports as f64 / 1_000_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentage_change() {
        assert_eq!(calculate_percentage_change(100.0, 150.0), 50.0);
        assert_eq!(calculate_percentage_change(100.0, 50.0), -50.0);
    }

    #[test]
    fn test_sol_lamports_conversion() {
        assert_eq!(sol_to_lamports(1.0), 1_000_000_000);
        assert_eq!(lamports_to_sol(1_000_000_000), 1.0);
    }
}
