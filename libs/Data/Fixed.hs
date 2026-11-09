module Data.Fixed (mod', div') where

import Base

div' :: (Real a, Integral b) => a -> a -> b
div' n d = floor (toRational n / toRational d)

mod' :: Real a => a -> a -> a
mod' n d = n - (fromInteger f) * d
  where f = div' n d
