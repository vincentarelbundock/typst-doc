"""A hand-written fixture exercising the Python reader: numpydoc sections,
annotations, defaults, a long signature, and a private helper."""


def mean_ci(x, level=0.95):
    """Compute a confidence interval.

    A longer description spanning
    two source lines.

    Parameters
    ----------
    x : array_like
        A numeric vector.
    level : float, optional
        Confidence level.

    Returns
    -------
    tuple
        Lower and upper bounds.

    Raises
    ------
    ValueError
        If ``x`` is empty.
    """


def convert(sourcevar, origin: str, destination: str, warn: bool = True, nomatch=None):
    """Convert codes between schemes.

    Parameters
    ----------
    sourcevar : array_like
        Codes to convert.
    """


def _helper(x):
    """Internal; must not appear in default output."""
