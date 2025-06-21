// Dynamic grid configuration
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GridConfig {
    pub rows: usize,
    pub cols: usize,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            rows: 8, // Default grid size
            cols: 12,
        }
    }
}

impl GridConfig {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self { rows, cols }
    }

    pub fn cell_count(&self) -> usize {
        self.rows * self.cols
    }
}
