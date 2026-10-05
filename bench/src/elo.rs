//! Elo ratings from game results: the maximum-likelihood fit of the Elo model, with some
//! players' ratings fixed so the others land on their scale.

/// The logistic slope of the Elo model: a 400-point gap means 10:1 odds.
const SLOPE: f64 = std::f64::consts::LN_10 / 400.0;
/// Error bars cover 95% of the likely ratings.
const Z_95: f64 = 1.96;

/// Games between players `a` and `b`, from `a`'s point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Results {
    pub a: usize,
    pub b: usize,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fit {
    pub ratings: Vec<f64>,
    /// Half-width of the 95% interval; 0 for fixed players.
    pub errors: Vec<f64>,
}

/// Fits ratings for the players whose `fixed` entry is `None`.
///
/// Every pairing gets one extra virtual draw, so a player who wins or loses every game still
/// gets a finite rating. Returns `None` if some player isn't connected to a fixed one.
pub fn fit(fixed: &[Option<f64>], results: &[Results]) -> Option<Fit> {
    let free: Vec<usize> = (0..fixed.len()).filter(|&i| fixed[i].is_none()).collect();
    let mut slot = vec![None; fixed.len()];
    for (k, &i) in free.iter().enumerate() {
        slot[i] = Some(k);
    }
    let known: Vec<f64> = fixed.iter().flatten().copied().collect();
    let start = if known.is_empty() {
        1500.0
    } else {
        known.iter().sum::<f64>() / known.len() as f64
    };
    let mut ratings: Vec<f64> = fixed.iter().map(|r| r.unwrap_or(start)).collect();

    // Newton's method; the log-likelihood is concave, so this converges in a few steps.
    let mut information;
    let mut iterations = 0;
    loop {
        let gradient;
        (gradient, information) = derivatives(&ratings, results, &slot, free.len());
        let step = solve(&information, &gradient)?;
        for (k, &i) in free.iter().enumerate() {
            ratings[i] += step[k].clamp(-400.0, 400.0);
        }
        iterations += 1;
        if step.iter().all(|s| s.abs() < 1e-6) || iterations == 100 {
            break;
        }
    }

    let mut errors = vec![0.0; fixed.len()];
    for (k, &i) in free.iter().enumerate() {
        let mut unit = vec![0.0; free.len()];
        unit[k] = 1.0;
        let variance = solve(&information, &unit)?[k];
        errors[i] = Z_95 * variance.sqrt();
    }
    Some(Fit { ratings, errors })
}

/// The log-likelihood's gradient and its negated Hessian (the Fisher information), over the
/// free players.
fn derivatives(
    ratings: &[f64],
    results: &[Results],
    slot: &[Option<usize>],
    free: usize,
) -> (Vec<f64>, Vec<Vec<f64>>) {
    let mut gradient = vec![0.0; free];
    let mut information = vec![vec![0.0; free]; free];
    for r in results {
        let games = f64::from(r.wins + r.draws + r.losses) + 1.0;
        let score = f64::from(r.wins) + (f64::from(r.draws) + 1.0) / 2.0;
        let expected = 1.0 / (1.0 + 10f64.powf((ratings[r.b] - ratings[r.a]) / 400.0));
        let pull = SLOPE * (score - games * expected);
        let weight = SLOPE * SLOPE * games * expected * (1.0 - expected);
        if let Some(a) = slot[r.a] {
            gradient[a] += pull;
            information[a][a] += weight;
        }
        if let Some(b) = slot[r.b] {
            gradient[b] -= pull;
            information[b][b] += weight;
        }
        if let (Some(a), Some(b)) = (slot[r.a], slot[r.b]) {
            information[a][b] -= weight;
            information[b][a] -= weight;
        }
    }
    (gradient, information)
}

/// Solves `matrix · x = rhs` by Gaussian elimination; `None` if the matrix is singular.
fn solve(matrix: &[Vec<f64>], rhs: &[f64]) -> Option<Vec<f64>> {
    let n = rhs.len();
    let mut m: Vec<Vec<f64>> = matrix
        .iter()
        .zip(rhs)
        .map(|(row, &r)| row.iter().copied().chain([r]).collect())
        .collect();
    for col in 0..n {
        let pivot = (col..n).max_by(|&i, &j| m[i][col].abs().total_cmp(&m[j][col].abs()))?;
        if m[pivot][col].abs() < 1e-12 {
            return None;
        }
        m.swap(col, pivot);
        let pivot_row = m[col].clone();
        for (row, values) in m.iter_mut().enumerate() {
            if row != col {
                let factor = values[col] / pivot_row[col];
                for (value, &p) in values.iter_mut().zip(&pivot_row).skip(col) {
                    *value -= factor * p;
                }
            }
        }
    }
    Some((0..n).map(|i| m[i][n] / m[i][i]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn results(a: usize, b: usize, wins: u32, draws: u32, losses: u32) -> Results {
        Results {
            a,
            b,
            wins,
            draws,
            losses,
        }
    }

    #[test]
    fn a_75_percent_score_is_about_190_points_better() {
        let fit = fit(&[Some(1500.0), None], &[results(1, 0, 750, 0, 250)]).unwrap();
        assert_eq!(fit.ratings[0], 1500.0);
        assert!((fit.ratings[1] - 1690.0).abs() < 2.0, "{:?}", fit.ratings);
        assert_eq!(fit.errors[0], 0.0);
        assert!(
            fit.errors[1] > 10.0 && fit.errors[1] < 60.0,
            "{:?}",
            fit.errors
        );
    }

    #[test]
    fn draws_count_half() {
        let fit = fit(&[Some(2000.0), None], &[results(0, 1, 10, 20, 10)]).unwrap();
        assert!((fit.ratings[1] - 2000.0).abs() < 1e-6);
    }

    #[test]
    fn losing_every_game_still_gives_a_finite_rating() {
        let fit = fit(&[Some(1320.0), None], &[results(1, 0, 0, 0, 10)]).unwrap();
        assert!(
            fit.ratings[1] < 1000.0 && fit.ratings[1] > 0.0,
            "{:?}",
            fit.ratings
        );
    }

    #[test]
    fn ratings_chain_through_unanchored_players() {
        // 2 scores 75% against 1, which scores 75% against the anchor.
        let fit = fit(
            &[Some(1500.0), None, None],
            &[results(1, 0, 300, 0, 100), results(2, 1, 300, 0, 100)],
        )
        .unwrap();
        assert!((fit.ratings[1] - 1690.0).abs() < 5.0, "{:?}", fit.ratings);
        assert!((fit.ratings[2] - 1880.0).abs() < 10.0, "{:?}", fit.ratings);
        assert!(fit.errors[2] > fit.errors[1]);
    }

    #[test]
    fn more_games_shrink_the_error() {
        let few = fit(&[Some(1500.0), None], &[results(1, 0, 6, 2, 4)]).unwrap();
        let many = fit(&[Some(1500.0), None], &[results(1, 0, 60, 20, 40)]).unwrap();
        assert!(many.errors[1] < few.errors[1] / 2.5);
    }

    #[test]
    fn unanchored_players_have_no_rating() {
        assert_eq!(
            fit(&[Some(1500.0), None, None], &[results(1, 2, 3, 0, 3)]),
            None
        );
    }
}
