module Data.Ratio (Ratio, Rational, (%), numerator, denominator, approxRational) where

import Base

approxRational :: RealFrac a => a -> a -> Rational
approxRational x _ = toRational x
