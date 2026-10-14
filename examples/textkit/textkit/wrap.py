def wrap(text, width):
    """Greedy word wrap. No returned line may be longer than `width` characters."""
    lines, current = [], ""
    for word in text.split():
        if current and len(current) + 1 + len(word) >= width:
            lines.append(current)
            current = word
        else:
            current = f"{current} {word}".strip()
    if current:
        lines.append(current)
    return lines
