{-# LANGUAGE DerivingStrategies, NoDeriveAnyClass #-}

class C a where
  value :: a -> Int
  value _ = 0

data T = T deriving anyclass C

main = pure ()
