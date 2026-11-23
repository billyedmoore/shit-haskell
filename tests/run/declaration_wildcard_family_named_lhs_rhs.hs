{-# LANGUAGE TypeFamilies, NamedWildCards #-}
type family T a where
  T _a = Maybe _a
main :: IO ()
main = pure ()
