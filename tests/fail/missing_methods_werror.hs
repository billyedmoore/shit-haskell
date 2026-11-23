{-# OPTIONS_GHC -Werror=missing-methods #-}

class Measure a where
  lengthOf :: a -> Int
  unitsOf :: a -> String
  unitsOf _ = "units"

instance Measure Int

main :: IO ()
main = pure ()
