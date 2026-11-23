{-# LANGUAGE TypeFamilies #-}
type family T a where
  T a = Maybe missing
main :: IO ()
main = pure ()
