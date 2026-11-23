module MeasureClass where

class Choose a where
  cost, estimate :: a -> Int
  cost = estimate
  estimate = cost
  {-# MINIMAL cost | estimate #-}
