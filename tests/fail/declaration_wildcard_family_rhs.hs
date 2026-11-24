{-# LANGUAGE TypeFamilies, PartialTypeSignatures #-}
type family T a where
  T a = Maybe _
main :: IO ()
main = pure ()
