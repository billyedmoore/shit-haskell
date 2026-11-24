{-# LANGUAGE TypeFamilies #-}
type family T a where
  T _ = Bool
main :: IO ()
main = pure ()
