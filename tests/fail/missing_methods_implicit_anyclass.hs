{-# OPTIONS_GHC -Werror=missing-methods #-}

{-# LANGUAGE DeriveAnyClass #-}
class Measure a where
  lengthOf :: a -> Int
  unitsOf :: a -> String
  unitsOf _ = "units"

data Box = Box deriving Measure

main :: IO ()
main = pure ()
