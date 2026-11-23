class Measure a where
  lengthOf :: a -> Int
  {-# MINIMAL lengthOf | lengthsOf #-}

main :: IO ()
main = pure ()
