/// Returns the number of visual lines a string occupies when wrapped at `text_width` columns.
pub fn wrapped_line_count(value: &str, text_width: usize) -> usize {
    if value.is_empty() {
        return 1;
    }

    value
        .split('\n')
        .map(|line| {
            let char_count = line.chars().count();
            if char_count == 0 {
                1
            } else {
                char_count.div_ceil(text_width)
            }
        })
        .sum::<usize>()
        .max(1)
}
