{-# OPTIONS_GHC -Werror=missing-methods #-}

{-# LANGUAGE DerivingStrategies, DeriveAnyClass #-}
class Measure a where
  lengthOf :: a -> Int
  unitsOf :: a -> String
  unitsOf _ = "units"

data Box = Box deriving anyclass Measure

main :: IO ()
main = pure ()
