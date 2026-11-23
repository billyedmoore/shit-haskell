{-# OPTIONS_GHC -Werror -Wno-missing-methods #-}

class Measure a where
  lengthOf :: a -> Int
  unitsOf :: a -> String
  unitsOf _ = "units"

instance Measure Int

main :: IO ()
main = putStrLn (unitsOf (4 :: Int))
