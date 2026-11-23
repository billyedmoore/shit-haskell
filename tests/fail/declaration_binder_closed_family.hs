{-# LANGUAGE TypeFamilies #-}
type family Broken a a where
  Broken a a = a
main :: IO ()
main = pure ()
