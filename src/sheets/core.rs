use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct ValueRange {
    /// The range the values cover, in A1 notation.
    /// For output, this range indicates the entire requested range,
    /// even though the values will exclude trailing rows and columns.
    /// When appending values, this field represents the range to search for a
    /// table, after which values will be appended.
    pub range: Option<String>,
    /// The data that was read or to be written.  This is an array of arrays,
    /// the outer array representing all the data and each inner array
    /// representing a major dimension. Each item in the inner array
    /// corresponds with one cell.
    ///
    /// For output, empty trailing rows and columns will not be included.
    ///
    /// For input, supported value types are: bool, string, and double.
    /// Null values will be skipped.
    /// To set a cell to an empty value, set the string value to an empty string.
    pub values: Option<Vec<Vec<String>>>,
    /// The major dimension of the values.
    ///
    /// For output, if the spreadsheet data is: `A1=1,B1=2,A2=3,B2=4`,
    /// then requesting `range=A1:B2,majorDimension=ROWS` will return
    /// `[[1,2],[3,4]]`,
    /// whereas requesting `range=A1:B2,majorDimension=COLUMNS` will return
    /// `[[1,3],[2,4]]`.
    ///
    /// For input, with `range=A1:B2,majorDimension=ROWS` then `[[1,2],[3,4]]`
    /// will set `A1=1,B1=2,A2=3,B2=4`. With `range=A1:B2,majorDimension=COLUMNS`
    /// then `[[1,2],[3,4]]` will set `A1=1,B1=3,A2=2,B2=4`.
    ///
    /// When writing, if this field is not set, it defaults to ROWS.
    #[serde(rename = "majorDimension")]
    pub major_dimension: Option<String>,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct UpdateValuesResponse {
    /// The number of columns where at least one cell in the column was updated.
    #[serde(rename = "updatedColumns")]
    pub updated_columns: Option<i32>,
    /// The range (in A1 notation) that updates were applied to.
    #[serde(rename = "updatedRange")]
    pub updated_range: Option<String>,
    /// The number of rows where at least one cell in the row was updated.
    #[serde(rename = "updatedRows")]
    pub updated_rows: Option<i32>,
    /// The values of the cells after updates were applied.
    /// This is only included if the request's `includeValuesInResponse` field
    /// was `true`.
    #[serde(rename = "updatedData")]
    pub updated_data: Option<ValueRange>,
    /// The spreadsheet the updates were applied to.
    #[serde(rename = "spreadsheetId")]
    pub spreadsheet_id: Option<String>,
    /// The number of cells updated.
    #[serde(rename = "updatedCells")]
    pub updated_cells: Option<i32>,
}
