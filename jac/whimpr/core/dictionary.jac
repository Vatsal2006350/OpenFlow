"""Local vocabulary rules migrated from openflow-core's dictionary module."""

import json;
import pathlib;
import string;


obj DictionaryEntry {
    has correct: str;
    has mishears: list[str] = [];
    has source: str = "manual";
}


obj DictionaryStore {
    has entries: list[DictionaryEntry] = [];

    def add(correct: str, mishears: list[str], source: str = "manual") -> None {
        for existing in self.entries {
            if existing.correct.casefold() == correct.casefold() {
                for mishear in mishears {
                    if not any(item.casefold() == mishear.casefold() for item in existing.mishears) {
                        existing.mishears.append(mishear);
                    }
                }
                return;
            }
        }
        self.entries.append(DictionaryEntry(correct=correct, mishears=mishears, source=source));
    }

    def remove(correct: str) -> bool {
        before = len(self.entries);
        self.entries = [entry for entry in self.entries if entry.correct.casefold() != correct.casefold()];
        return len(self.entries) != before;
    }

    def prefilter(utterance: str, max_entries: int) -> list[dict] {
        tokens = [strip_ascii_punctuation(token).casefold() for token in utterance.split()];
        tokens = [token for token in tokens if token];
        grams = tokens.copy();
        for index in range(max(0, len(tokens) - 1)) {
            grams.append(tokens[index] + tokens[index + 1]);
        }

        selected = [];
        if max_entries <= 0 {
            return selected;
        }
        for entry in self.entries {
            targets = [entry.correct.casefold()] + [word.casefold() for word in entry.mishears];
            if any(close(gram, target) for gram in grams for target in targets) {
                selected.append({"correct": entry.correct, "mishears": entry.mishears.copy()});
                if len(selected) >= max_entries {
                    break;
                }
            }
        }
        return selected;
    }

    def to_json() -> str {
        data = [
            {"correct": entry.correct, "mishears": entry.mishears, "source": entry.source}
            for entry in self.entries
        ];
        return json.dumps({"entries": data}, ensure_ascii=False, indent=2);
    }
}


def load_dictionary(path: str) -> DictionaryStore {
    try {
        data = json.loads(pathlib.Path(path).read_text(encoding="utf-8"));
        entries = [
            DictionaryEntry(
                correct=item["correct"],
                mishears=item.get("mishears", []),
                source=item.get("source", "manual")
            )
            for item in data.get("entries", [])
        ];
        return DictionaryStore(entries=entries);
    } except (OSError, ValueError, KeyError, TypeError) {
        return DictionaryStore();
    }
}


def save_dictionary(store: DictionaryStore, path: str) {
    destination = pathlib.Path(path);
    destination.parent.mkdir(parents=True, exist_ok=True);
    destination.write_text(store.to_json(), encoding="utf-8");
}


def strip_ascii_punctuation(token: str) -> str {
    start = 0;
    end = len(token);
    while start < end and token[start] in string.punctuation {
        start += 1;
    }
    while end > start and token[end - 1] in string.punctuation {
        end -= 1;
    }
    return token[start:end];
}


def close(left: str, right: str) -> bool {
    if left == right {
        return True;
    }
    longest = max(len(left), len(right));
    if longest == 0 {
        return False;
    }
    return levenshtein(left, right) / longest <= 0.34;
}


def levenshtein(left: str, right: str) -> int {
    previous = list(range(len(right) + 1));
    for left_index in range(1, len(left) + 1) {
        current = [left_index];
        for right_index in range(1, len(right) + 1) {
            insertion = current[right_index - 1] + 1;
            deletion = previous[right_index] + 1;
            substitution = previous[right_index - 1] + (0 if left[left_index - 1] == right[right_index - 1] else 1);
            current.append(min(insertion, deletion, substitution));
        }
        previous = current;
    }
    return previous[len(right)];
}
