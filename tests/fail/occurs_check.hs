selfApply f = f f

main :: IO ()
main = print (selfApply id `seq` ())
