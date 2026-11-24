{-# OPTIONS_GHC -Werror=missing-methods #-}

class Measure a where
  lengthOf :: a -> Int
  unitsOf :: a -> String
  {-# MINIMAL lengthOf #-}

instance Measure Int where
  lengthOf = id

main :: IO ()
main = print (lengthOf (4 :: Int))
