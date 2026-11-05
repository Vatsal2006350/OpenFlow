"""Conservative local rules for learning corrected dictated names."""

cl def unicode_letter_ranges() -> list[list[int]] {
    return [
        [65, 90], [97, 122], [192, 687], [880, 1327], [1328, 1423],
        [1424, 2047], [2304, 3071], [3584, 3711], [4256, 4351],
        [4608, 5119], [5120, 5759], [7680, 7935], [12352, 12543],
        [13312, 40959], [44032, 55203], [63744, 64255], [65313, 65338],
        [65345, 65370]
    ];
}

cl def unicode_digit_ranges() -> list[list[int]] {
    return [[48, 57], [1632, 1641], [1776, 1785], [2406, 2415], [65296, 65305]];
}

cl def in_ranges(char: str, ranges: list[list[int]]) -> bool {
    if not char { return False; }
    code = ord(char);
    for span in ranges { if code >= span[0] and code <= span[1] { return True; } }
    return False;
}

cl def unicode_letter(char: str) -> bool {
    return in_ranges(char, unicode_letter_ranges());
}

cl def unicode_alphanumeric(char: str) -> bool {
    return unicode_letter(char) or in_ranges(char, unicode_digit_ranges());
}

cl def word_tokens(text: str) -> list[str] {
    words: list[str] = [];
    for token in text.split() {
        word_chars: list[str] = [];
        for char in token { if unicode_alphanumeric(char) { word_chars.append(char); } }
        word = "".join(word_chars);
        if word { words.append(word); }
    }
    return words;
}

cl def all_letters(word: str) -> bool {
    if not word { return False; }
    for char in word { if not unicode_letter(char) { return False; } }
    return True;
}

cl def normalized_edit_distance(left: str, right: str) -> float {
    a: list[str] = list(left.lower());
    b: list[str] = list(right.lower());
    longest = max(len(a), len(b));
    if longest == 0 { return 1.0; }
    previous: list[int] = [];
    for index in range(len(b) + 1) { previous.append(index); }
    for row in range(1, len(a) + 1) {
        current: list[int] = [row];
        for column in range(1, len(b) + 1) {
            substitution = 0 if a[row - 1] == b[column - 1] else 1;
            current.append(min(
                previous[column] + 1,
                current[column - 1] + 1,
                previous[column - 1] + substitution
            ));
        }
        previous = current;
    }
    return previous[len(b)] / longest;
}

cl def:pub detect_correction(inserted: str, after: str) -> dict | None {
    before_words = word_tokens(inserted);
    after_words = word_tokens(after);
    before_lower: list[str] = [];
    after_lower: list[str] = [];
    for word in before_words { if word.lower() not in before_lower { before_lower.append(word.lower()); } }
    for word in after_words { if word.lower() not in after_lower { after_lower.append(word.lower()); } }

    removed: list[str] = [];
    added: list[str] = [];
    for word in before_words { if word.lower() not in after_lower { removed.append(word); } }
    for word in after_words { if word.lower() not in before_lower { added.append(word); } }
    if len(removed) != 1 or len(added) != 1 { return None; }

    mishear = removed[0];
    correct = added[0];
    common_words: list[str] = [
        "the", "and", "for", "are", "but", "not", "you", "your", "youre", "with", "this", "that",
        "have", "from", "they", "theyre", "their", "there", "would", "could", "should", "about",
        "then", "than", "them", "these", "those", "here", "were", "well", "will", "what", "when",
        "where", "which", "while", "into", "just", "like", "make", "made", "want", "some", "time",
        "know", "take", "come", "back", "good", "much", "also", "been", "over", "only", "more",
        "most", "very", "even", "such", "many", "does", "done", "same", "sure", "okay", "yeah",
        "hey", "hello", "please", "thanks", "thank", "message", "email", "text", "call"
    ];
    if len(mishear) < 3 or len(correct) < 3 { return None; }
    if not all_letters(mishear) or not all_letters(correct) { return None; }
    if correct.lower() == mishear.lower() { return None; }
    if correct.lower() in common_words or mishear.lower() in common_words { return None; }
    if not correct[0].isupper() { return None; }
    distance = normalized_edit_distance(mishear, correct);
    if distance <= 0.0 or distance > 0.6 { return None; }
    return {"mishear": mishear, "correct": correct};
}
