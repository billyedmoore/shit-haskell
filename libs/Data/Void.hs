module Data.Void (Void, absurd, vacuous) where

import Base

data Void

absurd :: Void -> a
absurd v = v `seq` error "Data.Void.absurd"

vacuous :: Functor f => f Void -> f a
vacuous = fmap absurd
