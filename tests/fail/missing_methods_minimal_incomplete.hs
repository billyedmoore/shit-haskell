{-# OPTIONS_GHC -w -Werror #-}

class Measure a where
  lengthOf, sizeOf :: a -> Int
  {-# MINIMAL lengthOf #-}

main :: IO ()
main = pure ()
