{-# LANGUAGE GADTs, TypeFamilies #-}

type family F where F = T
data T where
  Mk :: F

main :: IO ()
main = pure ()
