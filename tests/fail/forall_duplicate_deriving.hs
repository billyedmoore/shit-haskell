{-# LANGUAGE StandaloneDeriving #-}

data T a = T a
deriving instance forall a a. Eq a => Eq (T a)

main :: IO ()
main = pure ()
