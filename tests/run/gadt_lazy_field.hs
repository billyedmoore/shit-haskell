{-# LANGUAGE GADTs, ScopedTypeVariables, FlexibleContexts, StrictData #-}
-- GADT lazy field annotation `~Type` and given `Coercible` transitivity in case alternatives.
import Data.Coerce (Coercible, coerce)

data Box a where
  LazyBox :: ~Int -> a -> Box a
  CoerceBox :: Coercible b [a] => (a -> Int) -> Box b

unwrapBox :: Box a -> Int
unwrapBox (LazyBox _ _) = 99
unwrapBox (CoerceBox _) = 0

newtype Wrap = Wrap [Int]

sumCoerced :: Box b -> b -> Int
sumCoerced (CoerceBox (f :: a -> Int)) b = sum (map f (coerce b :: [a]))
sumCoerced (LazyBox n _) _ = n

main :: IO ()
main = do
  print (unwrapBox (LazyBox (error "strict!") ()))
  print (sumCoerced (CoerceBox (+ 1) :: Box Wrap) (Wrap [10, 20, 30]))
