{-# LANGUAGE StandaloneDeriving #-}

data T a = T a
deriving instance forall a. Eq a => Eq (T a)

main :: IO ()
main = print (T True == T False)
