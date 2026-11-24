{-# OPTIONS_GHC -Werror #-}

class Measure a where
  lengthOf :: a -> Int
  lengthOf _ = 4
  {-# MINIMAL #-}
  unitsOf :: a -> String
  unitsOf _ = "units"

instance Measure Int

main :: IO ()
main = print (lengthOf (2 :: Int))
