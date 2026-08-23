% A hand-written fixture exercising the constructs the R reader handles:
% math, links, lists, describe, tabular, conditionals, and examples.
\name{mean_ci}
\alias{mean_ci}
\title{Confidence Interval for a \emph{Mean}}
\usage{
mean_ci(x, level = 0.95)
}
\arguments{
  \item{x}{A numeric vector. Values of \code{NA} are dropped.}
  \item{level, alpha}{Confidence level, in \eqn{(0, 1)}.}
}
\description{
  Computes a confidence interval, following \link[stats]{t.test}.

  A second paragraph with a URL: \url{https://example.org} and 100.5 units.
}
\details{
  The interval is \deqn{\bar{x} \pm t_{\alpha/2} s / \sqrt{n}}

  \itemize{
    \item First item
    \item Second item with \strong{bold}
  }

  \describe{
    \item{normal}{The usual case.}
    \item{small n}{Uses a t distribution.}
  }

  \tabular{lr}{
    Name \tab Value \cr
    Alpha \tab 0.05 \cr
  }

  \ifelse{html}{click here}{see page 3}
}
\value{
  A numeric vector of length two.
}
\examples{
x <- rnorm(100)
mean_ci(x)
\dontrun{
mean_ci(x, level = 0.99)
}
}
\seealso{\code{\link{t.test}}}
