{-# LANGUAGE TypeFamilies #-}
type family T a where
  T a = Maybe a
main :: IO ()
main = pure ()
