{-# LANGUAGE GADTs #-}
data T where
  T :: (forall a. missing -> T)
main :: IO ()
main = pure ()
