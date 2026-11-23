{-# LANGUAGE NoDataKinds #-}

import Data.Proxy

value :: Proxy ('x')
value = Proxy

main :: IO ()
main = pure ()
