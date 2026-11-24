{-# LANGUAGE TypeFamilies, NoGADTSyntax #-}

data family F a
data instance F Int where
  Mk :: F Int

main :: IO ()
main = pure ()
