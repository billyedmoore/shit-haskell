{-# LANGUAGE StandaloneDeriving #-}

data T a = T a
deriving instance forall a. Eq a => Eq (T b)

main :: IO ()
main = pure ()
