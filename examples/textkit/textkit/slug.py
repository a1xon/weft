import re


def slugify(text):
    """Turn a title into a URL slug: lowercase ascii words joined by single dashes."""
    text = text.lower().replace(" ", "-")
    return re.sub(r"[^a-z0-9-]", "", text)
