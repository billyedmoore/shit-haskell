{-# LANGUAGE NoDataKinds #-}

import Data.Proxy
data D = MkD
value :: Proxy 'MkD
value = Proxy

main :: IO ()
main = pure ()
