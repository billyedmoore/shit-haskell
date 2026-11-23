{-# LANGUAGE GADTs #-}
data T where
  T :: forall a. Show missing => { field :: a } -> T
main :: IO ()
main = pure ()
