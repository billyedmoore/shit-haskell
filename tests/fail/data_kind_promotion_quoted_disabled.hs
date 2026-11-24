{-# LANGUAGE TemplateHaskell, NoDataKinds #-}

import Language.Haskell.TH (Q, Type)

value :: Q Type
value = [t| 'True |]

main :: IO ()
main = pure ()
