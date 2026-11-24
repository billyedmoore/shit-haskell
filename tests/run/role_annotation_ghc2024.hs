{-# LANGUAGE GHC2024 #-}

type role Box nominal
newtype Box a = Box a deriving stock Show

main :: IO ()
main = print (Box True)
